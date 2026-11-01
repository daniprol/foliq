//! Rust PDF extractors for the foliq benchmarks.
//!
//! `bench/tools.py` runs `foliq-bench-rust <tool> <path>` once per file and
//! reads the text from stdout. Two pure-Rust readers other than foliq's
//! pdf_oxide: `pdf-extract` (the most downloaded) and `lopdf` (the PDF object
//! library that pdf-extract is built on, with its own simple text extraction).

use std::process::ExitCode;

fn pdf_extract(path: &str) -> Result<String, String> {
    pdf_extract::extract_text(path).map_err(|error| error.to_string())
}

fn lopdf(path: &str) -> Result<String, String> {
    let document = lopdf::Document::load(path).map_err(|error| error.to_string())?;
    // Page numbers start at 1, like in a PDF viewer.
    let page_numbers: Vec<u32> = document.get_pages().keys().copied().collect();
    document.extract_text(&page_numbers).map_err(|error| error.to_string())
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().collect();
    if arguments.len() != 3 {
        eprintln!("usage: foliq-bench-rust <pdf-extract|lopdf> <path>");
        return ExitCode::from(2);
    }
    let result = match arguments[1].as_str() {
        "pdf-extract" => pdf_extract(&arguments[2]),
        "lopdf" => lopdf(&arguments[2]),
        other => Err(format!("unknown tool {other}")),
    };
    match result {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
