//! Markdown files are already in foliq's format: they are used as they are, so
//! line numbers in citations match the original file.

use foliq_core::{Parsed, Parser, Result, SourceEntry};

pub struct MarkdownParser;

impl Parser for MarkdownParser {
    fn name(&self) -> &str {
        "markdown"
    }

    fn extensions(&self) -> Vec<String> {
        super::extensions(&["md", "markdown", "mdx"])
    }

    fn parse(&self, bytes: &[u8], _entry: &SourceEntry) -> Result<Parsed> {
        Ok(Parsed::markdown(super::bytes_to_text(bytes)))
    }
}
