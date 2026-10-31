//! The built-in parsers. Each one turns a file format into Markdown.
//!
//! | Parser     | Extensions              | How                                        |
//! |------------|-------------------------|--------------------------------------------|
//! | `markdown` | md, markdown, mdx       | used as is                                  |
//! | `text`     | txt, text, rst          | used as is                                  |
//! | `html`     | html, htm, xhtml        | main content converted with `scraper`       |
//! | `docx`     | docx                    | Word XML read with `zip` + `quick-xml`      |
//! | `pptx`     | pptx                    | one section per slide, with speaker notes   |
//! | `xlsx`     | xlsx, xlsm, xls, ods    | one section per sheet, read with `calamine` |
//! | `pdf`      | pdf                     | text per page with `pdf_oxide`              |

mod docx;
mod html;
mod markdown;
mod office_xml;
mod pdf;
mod pptx;
mod text;
mod xlsx;

use std::sync::Arc;

pub use docx::DocxParser;
use foliq_core::Parser;
pub use html::HtmlParser;
pub use markdown::MarkdownParser;
pub use pdf::PdfParser;
pub use pptx::PptxParser;
pub use text::TextParser;
pub use xlsx::XlsxParser;

/// One of each built-in parser.
pub fn builtin_parsers() -> Vec<Arc<dyn Parser>> {
    vec![
        Arc::new(MarkdownParser),
        Arc::new(TextParser),
        Arc::new(HtmlParser),
        Arc::new(DocxParser),
        Arc::new(PptxParser),
        Arc::new(XlsxParser),
        Arc::new(PdfParser),
    ]
}

/// Turns bytes into text, replacing invalid UTF-8 instead of failing.
fn bytes_to_text(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    // Drop a byte order mark, and use `\n` line ends everywhere.
    text.trim_start_matches('\u{feff}').replace("\r\n", "\n")
}

fn extensions(list: &[&str]) -> Vec<String> {
    list.iter().map(|extension| extension.to_string()).collect()
}
