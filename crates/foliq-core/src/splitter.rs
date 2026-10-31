//! Splitters: cut a document into chunks.

use crate::{Chunk, Document, Result};

/// Cuts a document into [`Chunk`]s for search.
pub trait Splitter: Send + Sync {
    /// A short name like `"structure"`.
    fn name(&self) -> &str;

    /// Changes when the splitter's output would change (its options included).
    fn fingerprint(&self) -> String;

    /// Splits one document. Must be a pure function of the document.
    fn split(&self, document: &Document) -> Result<Vec<Chunk>>;
}
