//! Stores: where documents and chunks are saved and searched.
//!
//! A store is two-way: the engine writes to it during a sync, and the agent
//! tools (outline, read, search, context) read from it. The tools never touch
//! a database directly, so the store can be swapped (SQLite today, Postgres later).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{Chunk, Document, Metadata, Result};

/// A named group of documents from one source, like `notes` or `contracts`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    pub name: String,
    /// The kind of source: `"folder"`.
    pub source: String,
    /// Where the source is: a folder path, a bucket URL...
    pub location: String,
    /// Who may see the collection's documents. Empty means everyone.
    pub access: Vec<String>,
}

/// What the store remembers about an indexed file, to skip unchanged files.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedVersion {
    pub document_id: String,
    pub version: String,
    pub content_hash: String,
    /// The parser and splitter fingerprints the document was processed with.
    pub fingerprint: String,
}

/// Which documents a read or a search may look at.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    /// Only these collections. Empty means all.
    pub collections: Vec<String>,
    /// Only documents whose path starts with this.
    pub path_prefix: Option<String>,
    /// Who is asking. `None` means "don't check access" (a trusted caller).
    pub principals: Option<Vec<String>>,
}

/// A keyword search.
#[derive(Debug, Clone, Default)]
pub struct SearchRequest {
    pub query: String,
    pub limit: usize,
    /// Applied before ranking, so results never leak what the caller can't see.
    pub filter: Filter,
}

/// One search result: a chunk, its document and its score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub chunk: Chunk,
    pub collection: String,
    pub path: String,
    pub title: String,
    /// Higher is better. Only comparable within one search.
    pub score: f64,
}

/// A short description of a stored document (without its text).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentInfo {
    pub id: String,
    pub collection: String,
    pub path: String,
    pub title: String,
    pub token_count: usize,
    pub chunk_count: usize,
    pub metadata: Metadata,
}

/// Saves documents and chunks, and finds them again.
pub trait Store: Send + Sync {
    /// Adds or replaces a collection.
    fn save_collection(&self, collection: &Collection) -> Result<()>;

    /// Removes a collection and all its documents.
    fn remove_collection(&self, name: &str) -> Result<()>;

    /// All collections.
    fn collections(&self) -> Result<Vec<Collection>>;

    /// What was indexed for each path of a collection (path -> version).
    fn indexed_versions(&self, collection: &str) -> Result<HashMap<String, IndexedVersion>>;

    /// Saves a document and its chunks, replacing any older copy, all at once.
    fn write_document(&self, document: &Document, chunks: &[Chunk], fingerprint: &str) -> Result<()>;

    /// Records a new version for a document whose content did not change.
    fn update_version(&self, document_id: &str, version: &str) -> Result<()>;

    /// Deletes a document and its chunks.
    fn delete_document(&self, document_id: &str) -> Result<()>;

    /// Loads a whole document, or `None` if there is none at this path.
    fn document(&self, collection: &str, path: &str) -> Result<Option<Document>>;

    /// Lists documents that pass the filter.
    fn documents(&self, filter: &Filter) -> Result<Vec<DocumentInfo>>;

    /// All chunks of the documents that pass the filter, in document order.
    fn chunks(&self, filter: &Filter) -> Result<Vec<Chunk>>;

    /// Keyword search (BM25 in the built-in store), best results first.
    fn search(&self, request: &SearchRequest) -> Result<Vec<SearchHit>>;
}
