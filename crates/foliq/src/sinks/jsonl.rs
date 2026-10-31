//! Appends every change to a JSON Lines file (one JSON object per line).
//!
//! Handy to feed another system (a vector database, a search service) without
//! writing a plugin: read the file, apply the lines, truncate it.

use std::io::Write;
use std::path::PathBuf;

use foliq_core::{Change, Result, Sink};

pub struct JsonlSink {
    path: PathBuf,
}

impl JsonlSink {
    pub fn new(path: impl Into<PathBuf>) -> JsonlSink {
        JsonlSink { path: path.into() }
    }
}

impl Sink for JsonlSink {
    fn name(&self) -> &str {
        "jsonl"
    }

    fn apply(&self, changes: &[Change]) -> Result<()> {
        if changes.is_empty() {
            return Ok(());
        }
        if let Some(folder) = self.path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&self.path)?;
        let mut lines = String::new();
        for change in changes {
            // Serializing our own types can't fail, so `expect` is safe here.
            lines.push_str(&serde_json::to_string(change).expect("changes serialize to JSON"));
            lines.push('\n');
        }
        file.write_all(lines.as_bytes())?;
        Ok(())
    }
}
