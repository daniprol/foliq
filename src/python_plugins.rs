//! Python classes used as foliq plugins.
//!
//! Each adapter holds a Python object and implements the matching Rust trait,
//! so the engine treats it like any built-in plugin. Every call takes the
//! Python lock (the GIL) with `Python::attach`, so the engine can call
//! plugins from many threads safely.
//!
//! What a Python plugin looks like (see `foliq/plugins.py` for the protocols):
//!
//! | Plugin   | Needs                                                             |
//! |----------|-------------------------------------------------------------------|
//! | parser   | `extensions: list[str]`, `parse(data: bytes, path: str) -> str \| dict` |
//! | splitter | `split(text: str) -> list[str]` (pieces of the text, in order)     |
//! | sink     | `apply(changes: list[dict]) -> None`                               |
//!
//! All may have `name: str` and `version: str`. Changing `version` makes foliq
//! re-process the files the plugin handled.

use foliq::splitter::make_chunk;
use foliq::{Change, Chunk, Document, Error, Metadata, Parsed, Parser, Result, Section, Sink, SourceEntry, Splitter};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyString};

use crate::json_convert::{json_to_python, python_to_json};

/// The name and version of a Python plugin, read once when it is registered.
struct PluginInfo {
    name: String,
    version: String,
}

impl PluginInfo {
    fn read(object: &Bound<'_, PyAny>) -> PyResult<PluginInfo> {
        let name = match object.getattr_opt("name")? {
            Some(name) => name.extract()?,
            None => object.get_type().name()?.to_string(),
        };
        let version = match object.getattr_opt("version")? {
            Some(version) => version.str()?.to_string(),
            None => "1".to_string(),
        };
        Ok(PluginInfo { name, version })
    }

    fn fingerprint(&self) -> String {
        format!("python:{}@{}", self.name, self.version)
    }

    /// Wraps a Python exception as a plugin error, keeping its type and message.
    fn error(&self, py: Python<'_>, error: PyErr) -> Error {
        Error::plugin(&self.name, format_python_error(py, &error))
    }
}

fn format_python_error(py: Python<'_>, error: &PyErr) -> String {
    let kind = error
        .get_type(py)
        .name()
        .map(|name| name.to_string())
        .unwrap_or_else(|_| "Exception".into());
    format!("{kind}: {}", error.value(py))
}

/// A Python object used as a [`Parser`].
pub struct PythonParser {
    info: PluginInfo,
    extensions: Vec<String>,
    object: Py<PyAny>,
}

impl PythonParser {
    pub fn new(object: &Bound<'_, PyAny>) -> PyResult<PythonParser> {
        let extensions: Vec<String> = object.getattr("extensions")?.extract()?;
        Ok(PythonParser {
            info: PluginInfo::read(object)?,
            extensions: extensions
                .iter()
                .map(|extension| extension.trim_start_matches('.').to_lowercase())
                .collect(),
            object: object.clone().unbind(),
        })
    }
}

impl Parser for PythonParser {
    fn name(&self) -> &str {
        &self.info.name
    }

    fn fingerprint(&self) -> String {
        self.info.fingerprint()
    }

    fn extensions(&self) -> Vec<String> {
        self.extensions.clone()
    }

    fn parse(&self, bytes: &[u8], entry: &SourceEntry) -> Result<Parsed> {
        Python::attach(|py| {
            let result = self
                .object
                .call_method1(py, "parse", (PyBytes::new(py, bytes), &entry.path))
                .map_err(|error| self.info.error(py, error))?;
            parsed_from_python(result.bind(py)).map_err(|error| self.info.error(py, error))
        })
    }
}

/// A parser may return the Markdown text, or a dict with `text`, `title` and `metadata`.
fn parsed_from_python(result: &Bound<'_, PyAny>) -> PyResult<Parsed> {
    if let Ok(text) = result.cast::<PyString>() {
        return Ok(Parsed::markdown(text.to_str()?.to_owned()));
    }
    let fields = result.cast::<PyDict>()?;
    let text: String = fields
        .get_item("text")?
        .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("a parser's dict needs a 'text' key"))?
        .extract()?;
    let title: Option<String> = fields.get_item("title")?.map(|title| title.extract()).transpose()?;
    let metadata: Metadata = match fields.get_item("metadata")? {
        Some(metadata) => match python_to_json(&metadata)? {
            serde_json::Value::Object(map) => map,
            _ => return Err(pyo3::exceptions::PyTypeError::new_err("'metadata' must be a dict")),
        },
        None => Metadata::new(),
    };
    Ok(Parsed {
        text,
        title,
        elements: None,
        metadata,
    })
}

/// A Python object used as a [`Splitter`].
///
/// It returns pieces of text; foliq finds each piece in the document (in
/// order) and adds the positions, section and heading path itself.
pub struct PythonSplitter {
    info: PluginInfo,
    object: Py<PyAny>,
}

impl PythonSplitter {
    pub fn new(object: &Bound<'_, PyAny>) -> PyResult<PythonSplitter> {
        Ok(PythonSplitter {
            info: PluginInfo::read(object)?,
            object: object.clone().unbind(),
        })
    }
}

impl Splitter for PythonSplitter {
    fn name(&self) -> &str {
        &self.info.name
    }

    fn fingerprint(&self) -> String {
        self.info.fingerprint()
    }

    fn split(&self, document: &Document) -> Result<Vec<Chunk>> {
        let pieces: Vec<String> = Python::attach(|py| {
            self.object
                .call_method1(py, "split", (&document.text,))
                .and_then(|result| result.extract(py))
                .map_err(|error| self.info.error(py, error))
        })?;

        let whole_document = Section {
            number: "0".to_string(),
            title: document.title.clone(),
            level: 0,
            heading_path: Vec::new(),
            start: 0,
            end: document.text.len(),
            own_end: document.text.len(),
        };
        let mut chunks = Vec::new();
        let mut cursor = 0;
        for piece in pieces.iter().filter(|piece| !piece.trim().is_empty()) {
            // Look after the previous piece first, then anywhere (pieces may overlap).
            let start = document.text[cursor..]
                .find(piece.as_str())
                .map(|offset| cursor + offset)
                .or_else(|| document.text.find(piece.as_str()))
                .ok_or_else(|| {
                    let preview: String = piece.chars().take(60).collect();
                    Error::plugin(
                        &self.info.name,
                        format!("piece not found in the document text: {preview:?}"),
                    )
                })?;
            cursor = start + piece.len();
            // Leave out the whitespace around the piece, like the built-in splitter.
            let start = start + (piece.len() - piece.trim_start().len());
            let end = start + piece.trim().len();
            let section = document.section_at(start).unwrap_or_else(|| whole_document.clone());
            chunks.push(make_chunk(document, chunks.len(), start, end, &section));
        }
        Ok(chunks)
    }
}

/// A Python object used as a [`Sink`]. It gets each change as a dict.
pub struct PythonSink {
    info: PluginInfo,
    object: Py<PyAny>,
}

impl PythonSink {
    pub fn new(object: &Bound<'_, PyAny>) -> PyResult<PythonSink> {
        Ok(PythonSink {
            info: PluginInfo::read(object)?,
            object: object.clone().unbind(),
        })
    }
}

impl Sink for PythonSink {
    fn name(&self) -> &str {
        &self.info.name
    }

    fn apply(&self, changes: &[Change]) -> Result<()> {
        let json = serde_json::to_value(changes).map_err(|error| Error::plugin(&self.info.name, error.to_string()))?;
        Python::attach(|py| {
            let changes = json_to_python(py, &json).map_err(|error| self.info.error(py, error))?;
            self.object
                .call_method1(py, "apply", (changes,))
                .map_err(|error| self.info.error(py, error))?;
            Ok(())
        })
    }
}
