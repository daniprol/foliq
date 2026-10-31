//! Sources: where documents come from (a folder today; S3, Azure Blob... later).

use serde::{Deserialize, Serialize};

use crate::{Metadata, Result};

/// One file a source can read.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceEntry {
    /// The path inside the source, with `/` separators: `contracts/acme.pdf`.
    pub path: String,
    /// Where the file lives: `file:///...`, `s3://bucket/key`...
    pub uri: String,
    /// The source's own version of the file: a modified time and size, an ETag...
    /// foliq only reads files whose version changed since the last sync.
    pub version: String,
    /// The size in bytes.
    pub size: u64,
    /// Who may see the file. Empty means "use the collection's access list".
    pub access: Vec<String>,
    /// Extra information to copy onto the document.
    pub metadata: Metadata,
}

/// A place that holds documents.
///
/// `Send + Sync` means the value can be shared between threads (foliq parses
/// files in parallel). Most plugins get it for free.
pub trait Source: Send + Sync {
    /// A short name for messages, like `"folder"`.
    fn name(&self) -> &str;

    /// Lists every file, with its current version. Must be cheap: no file contents.
    fn list(&self) -> Result<Vec<SourceEntry>>;

    /// Reads one file's bytes.
    fn read(&self, entry: &SourceEntry) -> Result<Vec<u8>>;
}
