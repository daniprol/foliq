//! PDF files with a text layer, read with the pure-Rust `pdf_oxide` crate.
//!
//! Each page is converted to Markdown (headings are guessed from font sizes)
//! and starts with a `<!-- page N -->` marker, so every chunk knows its pages.
//! Scanned PDFs (images without text) need an OCR parser plugin.

use foliq_core::{Error, Parsed, Parser, Result, SourceEntry};
use pdf_oxide::PdfDocument;
use pdf_oxide::converters::ConversionOptions;

use crate::markdown::MarkdownWriter;

pub struct PdfParser;

impl Parser for PdfParser {
    fn name(&self) -> &str {
        "pdf"
    }

    fn extensions(&self) -> Vec<String> {
        super::extensions(&["pdf"])
    }

    fn parse(&self, bytes: &[u8], entry: &SourceEntry) -> Result<Parsed> {
        let fail = |error: pdf_oxide::Error| Error::parse(&entry.path, error);
        let document = PdfDocument::from_bytes(bytes.to_vec()).map_err(fail)?;
        let options = ConversionOptions {
            detect_headings: true,
            // Not `strip_running_headers_footers`: pdf_oxide 0.3 rescans every
            // page for each page it converts, so a 350-page book took minutes
            // instead of about a second (see benchmarks/).
            ..ConversionOptions::default()
        };

        let mut writer = MarkdownWriter::new();
        for page_index in 0..document.page_count().map_err(fail)? {
            writer.page("page", page_index as u32 + 1);
            // One unreadable page should not lose the whole document.
            match document.to_markdown(page_index, &options) {
                Ok(markdown) => writer.raw(&markdown),
                Err(error) => writer.paragraph(&format!("[page {} could not be read: {error}]", page_index + 1)),
            }
        }
        Ok(Parsed::markdown(writer.finish()))
    }
}
