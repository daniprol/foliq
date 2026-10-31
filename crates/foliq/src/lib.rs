//! The foliq engine: built-in parsers, the folder source, the structure splitter,
//! the SQLite store, sinks, and the `Index` that ties them together.

mod index;
pub mod markdown;
pub mod parsers;
pub mod sinks;
pub mod sources;
pub mod splitter;
pub mod store;
pub mod tokens;

pub use foliq_core::*;
pub use index::{
    CollectionStatus, Context, Index, IndexBuilder, Outline, OutlineSection, Passage, SearchOptions, SearchResult,
    SyncFailure, SyncReport, citation, document_id,
};
