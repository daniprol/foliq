//! Sinks: receive every change after a sync, to export it somewhere else.
//!
//! A sink is one-way. Use it to copy chunks into your own vector database, a
//! JSONL file or a search service. The text mirror (`.foliq/text`) is a sink too.

use serde::Serialize;

use crate::{Chunk, Document, Result};

/// One change found by a sync.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
// `Upsert` is much bigger than `Delete`. That's fine: a sync holds few changes at
// once, and boxing the document would only make the type harder to use.
#[allow(clippy::large_enum_variant)]
pub enum Change {
    /// A new or changed document, with all its chunks (replace the old ones).
    Upsert { document: Document, chunks: Vec<Chunk> },
    /// A document that disappeared from its source.
    Delete {
        document_id: String,
        collection: String,
        path: String,
    },
}

/// Receives the changes of each sync, in one batch.
pub trait Sink: Send + Sync {
    /// A short name like `"jsonl"`.
    fn name(&self) -> &str;

    /// Applies a batch of changes. Must be safe to run twice with the same changes.
    fn apply(&self, changes: &[Change]) -> Result<()>;
}
