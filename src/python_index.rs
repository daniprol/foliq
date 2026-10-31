//! `foliq.Index`: the Python face of the Rust [`foliq::Index`].
//!
//! Results come back as plain Python dicts and lists (JSON-like), so they can
//! go straight into a prompt, a tool result or `json.dumps`. Their keys are
//! described with `TypedDict`s in `foliq/_core.pyi`.
//!
//! Long work (sync, search...) runs with the GIL released (`py.detach`), so
//! other Python threads keep running.

use std::path::PathBuf;
use std::sync::Arc;

use foliq::splitter::StructureSplitter;
use foliq::{Filter, Index, SearchOptions};
use pyo3::prelude::*;
use pyo3::types::PyString;
use serde::Serialize;

use crate::errors::to_python_error;
use crate::json_convert::json_to_python;
use crate::python_plugins::{PythonParser, PythonSink, PythonSplitter};

/// A searchable index of documents, saved in one folder (`.foliq` by default).
#[pyclass(module = "foliq", name = "Index", subclass, frozen)]
pub struct PythonIndex {
    index: Index,
    folder: PathBuf,
}

#[pymethods]
impl PythonIndex {
    #[new]
    #[pyo3(signature = (path = PathBuf::from(".foliq"), *, parsers = Vec::new(), splitter = None, sinks = Vec::new(), max_tokens = 400, mirror = true))]
    fn new(
        path: PathBuf,
        parsers: Vec<Bound<'_, PyAny>>,
        splitter: Option<Bound<'_, PyAny>>,
        sinks: Vec<Bound<'_, PyAny>>,
        max_tokens: usize,
        mirror: bool,
    ) -> PyResult<PythonIndex> {
        let mut builder = Index::builder(&path).mirror(mirror);
        for parser in &parsers {
            builder = builder.parser(Arc::new(PythonParser::new(parser)?));
        }
        builder = match splitter {
            Some(splitter) => builder.splitter(Arc::new(PythonSplitter::new(&splitter)?)),
            None => builder.splitter(Arc::new(StructureSplitter { max_tokens })),
        };
        for sink in &sinks {
            builder = builder.sink(Arc::new(PythonSink::new(sink)?));
        }
        let index = builder.open().map_err(to_python_error)?;
        Ok(PythonIndex { index, folder: path })
    }

    fn __repr__(&self) -> String {
        format!("Index({:?})", self.folder.display().to_string())
    }

    /// The folder that holds the index.
    #[getter]
    fn path(&self) -> PathBuf {
        self.folder.clone()
    }

    /// Adds a folder as a collection (or updates it). Call `sync()` to read its files.
    #[pyo3(signature = (folder, *, collection = None, access = Vec::new()))]
    fn add(
        &self,
        py: Python<'_>,
        folder: PathBuf,
        collection: Option<String>,
        access: Vec<String>,
    ) -> PyResult<Py<PyAny>> {
        let added = self
            .index
            .add_folder(&folder, collection.as_deref(), access)
            .map_err(to_python_error)?;
        to_python(py, &added)
    }

    /// Removes a collection and its documents. Returns False if there was none.
    fn remove(&self, collection: &str) -> PyResult<bool> {
        self.index.remove(collection).map_err(to_python_error)
    }

    /// Reads new and changed files, and forgets deleted ones.
    fn sync(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let report = py.detach(|| self.index.sync()).map_err(to_python_error)?;
        to_python(py, &report)
    }

    /// All collections.
    fn collections(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        to_python(py, &self.index.collections().map_err(to_python_error)?)
    }

    /// Documents, chunks and tokens per collection.
    fn status(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        to_python(py, &self.index.status().map_err(to_python_error)?)
    }

    /// The documents (without their text) that pass the filters.
    #[pyo3(signature = (*, collections = Vec::new(), path_prefix = None, principal = None))]
    fn documents(
        &self,
        py: Python<'_>,
        collections: Vec<String>,
        path_prefix: Option<String>,
        principal: Option<Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let filter = Filter {
            collections,
            path_prefix,
            principals: principals(principal)?,
        };
        to_python(py, &self.index.documents(&filter).map_err(to_python_error)?)
    }

    /// The sections of a document, with their numbers and sizes in tokens.
    #[pyo3(signature = (reference, *, principal = None))]
    fn outline(&self, py: Python<'_>, reference: &str, principal: Option<Bound<'_, PyAny>>) -> PyResult<Py<PyAny>> {
        let principals = principals(principal)?;
        let outline = py
            .detach(|| self.index.outline(reference, principals.as_deref()))
            .map_err(to_python_error)?;
        to_python(py, &outline)
    }

    /// Reads a document, a section (`"doc.md#2.3"`) or pages (`"doc.pdf#p4-6"`).
    #[pyo3(signature = (reference, *, budget = None, principal = None))]
    fn read(
        &self,
        py: Python<'_>,
        reference: &str,
        budget: Option<usize>,
        principal: Option<Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let principals = principals(principal)?;
        let passage = py
            .detach(|| self.index.read(reference, budget, principals.as_deref()))
            .map_err(to_python_error)?;
        to_python(py, &passage)
    }

    /// Keyword search (BM25). Returns chunks with citations, best first.
    #[pyo3(signature = (query, *, limit = 10, collections = Vec::new(), path_prefix = None, principal = None))]
    fn search(
        &self,
        py: Python<'_>,
        query: &str,
        limit: usize,
        collections: Vec<String>,
        path_prefix: Option<String>,
        principal: Option<Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let options = SearchOptions {
            limit,
            collections,
            path_prefix,
            principals: principals(principal)?,
        };
        let results = py
            .detach(|| self.index.search(query, &options))
            .map_err(to_python_error)?;
        to_python(py, &results)
    }

    /// The best passages for a question, cited and packed into `budget` tokens.
    #[pyo3(signature = (query, *, budget = 2000, limit = 10, collections = Vec::new(), path_prefix = None, principal = None))]
    #[allow(clippy::too_many_arguments)]
    fn context(
        &self,
        py: Python<'_>,
        query: &str,
        budget: usize,
        limit: usize,
        collections: Vec<String>,
        path_prefix: Option<String>,
        principal: Option<Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let options = SearchOptions {
            limit,
            collections,
            path_prefix,
            principals: principals(principal)?,
        };
        let context = py
            .detach(|| self.index.context(query, budget, &options))
            .map_err(to_python_error)?;
        to_python(py, &context)
    }

    /// Every chunk that passes the filters, in document order (to export or embed them).
    #[pyo3(signature = (*, collections = Vec::new(), path_prefix = None, principal = None))]
    fn chunks(
        &self,
        py: Python<'_>,
        collections: Vec<String>,
        path_prefix: Option<String>,
        principal: Option<Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let filter = Filter {
            collections,
            path_prefix,
            principals: principals(principal)?,
        };
        let chunks = py
            .detach(|| self.index.store().chunks(&filter))
            .map_err(to_python_error)?;
        to_python(py, &chunks)
    }

    /// Tools for an AI agent (outline, read, search, context), bound to one principal.
    ///
    /// The principal is fixed here and never becomes a tool argument, so the
    /// model can't ask to see someone else's documents.
    #[pyo3(signature = (*, principal = None))]
    fn tools<'py>(slf: &Bound<'py, Self>, principal: Option<Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyAny>> {
        slf.py()
            .import("foliq.tools")?
            .call_method1("make_tools", (slf, principal))
    }
}

/// `principal` may be one string (`"user:ana"`) or a list of them.
fn principals(principal: Option<Bound<'_, PyAny>>) -> PyResult<Option<Vec<String>>> {
    match principal {
        None => Ok(None),
        Some(value) if value.is_none() => Ok(None),
        Some(value) => match value.cast::<PyString>() {
            Ok(text) => Ok(Some(vec![text.to_str()?.to_owned()])),
            Err(_) => Ok(Some(value.extract()?)),
        },
    }
}

/// Any result -> plain Python dicts and lists.
fn to_python(py: Python<'_>, value: &impl Serialize) -> PyResult<Py<PyAny>> {
    let json = serde_json::to_value(value).map_err(|error| crate::errors::FoliqError::new_err(error.to_string()))?;
    Ok(json_to_python(py, &json)?.unbind())
}
