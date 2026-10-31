//! Plain text files, used as they are.

use foliq_core::{Parsed, Parser, Result, SourceEntry};

pub struct TextParser;

impl Parser for TextParser {
    fn name(&self) -> &str {
        "text"
    }

    fn extensions(&self) -> Vec<String> {
        super::extensions(&["txt", "text", "rst"])
    }

    fn parse(&self, bytes: &[u8], _entry: &SourceEntry) -> Result<Parsed> {
        Ok(Parsed::markdown(super::bytes_to_text(bytes)))
    }
}
