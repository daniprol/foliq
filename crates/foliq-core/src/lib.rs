//! The types and plugin interfaces of foliq.
//!
//! foliq is a small engine with sockets. Every socket is a Rust *trait* (like a
//! Python `Protocol` or abstract base class): a list of methods a plugin must
//! have. The built-in plugins and your own plugins implement the same traits,
//! so the engine can't tell them apart.
//!
//! | Socket     | Trait      | What it does                                         |
//! |------------|------------|------------------------------------------------------|
//! | Source     | [`Source`]   | Lists files (with a version each) and reads their bytes |
//! | Parser     | [`Parser`]   | Turns bytes into a [`Document`] (Markdown text + structure) |
//! | Splitter   | [`Splitter`] | Cuts a document into [`Chunk`]s                     |
//! | Store      | [`Store`]    | Saves documents and chunks, and searches them        |
//! | Sink       | [`Sink`]     | Receives every change after a sync (exports)         |
//! | Embedder   | [`Embedder`] | Turns texts into vectors (used from foliq 0.2)       |
//!
//! Depend on this crate (not on `foliq`) to write a plugin: it is small and
//! changes rarely.

mod chunk;
mod document;
mod embedder;
mod error;
mod parser;
mod sink;
mod source;
mod splitter;
mod store;

pub use chunk::Chunk;
pub use document::{Document, Element, ElementKind, Section};
pub use embedder::Embedder;
pub use error::{Error, Result};
pub use parser::{Parsed, Parser};
pub use sink::{Change, Sink};
pub use source::{Source, SourceEntry};
pub use splitter::Splitter;
pub use store::{Collection, DocumentInfo, Filter, IndexedVersion, SearchHit, SearchRequest, Store};

/// A JSON object with free-form extra information, like a Python `dict[str, Any]`.
pub type Metadata = serde_json::Map<String, serde_json::Value>;
