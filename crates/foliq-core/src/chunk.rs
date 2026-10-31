//! A chunk: a small piece of a document that search can find and cite.

use serde::{Deserialize, Serialize};

/// A piece of a document, with everything needed to cite it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chunk {
    /// `"{document_id}:{ordinal}"`.
    pub id: String,
    pub document_id: String,
    /// The chunk's position in its document: 0, 1, 2...
    pub ordinal: usize,
    /// Byte range in the document's text.
    pub start: usize,
    pub end: usize,
    /// Line range in the document's text (counting from 1).
    pub line_start: usize,
    pub line_end: usize,
    /// Page (or slide) range, for formats with pages.
    pub page_start: Option<u32>,
    pub page_end: Option<u32>,
    /// The number of the section the chunk belongs to (`"2.3"`).
    pub section: String,
    /// The headings above the chunk: `["Billing", "Refunds"]`.
    pub heading_path: Vec<String>,
    /// The size of `text` in tokens.
    pub token_count: usize,
    /// The chunk's text: `document.text[start..end]`.
    pub text: String,
    /// A hash of `text`. Two chunks with the same text share it, so work done
    /// on one (like computing its embedding) can be reused for the other.
    pub content_key: String,
}
