//! The built-in sinks: they receive every change after a sync.
//!
//! | Sink         | What it does                                                        |
//! |--------------|---------------------------------------------------------------------|
//! | `TextMirror` | writes each non-Markdown document as Markdown under `.foliq/text`  |
//! | `JsonlSink`  | appends every change as one JSON line to a file                     |

mod jsonl;
mod mirror;

pub use jsonl::JsonlSink;
pub use mirror::TextMirror;
