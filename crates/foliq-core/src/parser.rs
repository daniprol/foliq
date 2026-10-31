//! Parsers: turn a file's bytes into Markdown text and structure.

use crate::{Element, Metadata, Result, SourceEntry};

/// What a parser returns.
///
/// The simplest parser only fills `text` with Markdown: foliq finds the
/// headings, tables and `<!-- page N -->` markers in it by itself. A parser
/// that knows more (positions, a richer structure) can fill `elements` too.
#[derive(Debug, Clone, Default)]
pub struct Parsed {
    /// The document as Markdown.
    pub text: String,
    /// The document's title, if the file has one.
    pub title: Option<String>,
    /// The structure. `None` lets foliq read it from the Markdown.
    pub elements: Option<Vec<Element>>,
    pub metadata: Metadata,
}

impl Parsed {
    /// A parse result with just Markdown text.
    pub fn markdown(text: String) -> Parsed {
        Parsed {
            text,
            ..Parsed::default()
        }
    }
}

/// Turns the bytes of one kind of file into a [`Parsed`] document.
pub trait Parser: Send + Sync {
    /// A short, stable name like `"pdf"` or `"docling"`.
    fn name(&self) -> &str;

    /// Changes when the parser's output would change (a new version, other options).
    /// Documents parsed with an older fingerprint are parsed again.
    fn fingerprint(&self) -> String {
        format!("{}@1", self.name())
    }

    /// The file extensions this parser reads, lowercase and without the dot: `["pdf"]`.
    fn extensions(&self) -> Vec<String>;

    /// Parses one file.
    fn parse(&self, bytes: &[u8], entry: &SourceEntry) -> Result<Parsed>;
}
