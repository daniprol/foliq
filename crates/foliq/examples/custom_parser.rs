//! A Rust plugin: a parser for `.csv` files, used by an index.
//!
//! Run it with `cargo run -p foliq --example custom_parser -- <folder>`.

use std::sync::Arc;

use foliq::{Index, Parsed, Parser, Result, SearchOptions, SourceEntry};

/// Turns CSV files into Markdown tables.
struct CsvParser;

impl Parser for CsvParser {
    fn name(&self) -> &str {
        "csv"
    }

    fn extensions(&self) -> Vec<String> {
        vec!["csv".to_string()]
    }

    fn parse(&self, bytes: &[u8], _entry: &SourceEntry) -> Result<Parsed> {
        let text = String::from_utf8_lossy(bytes);
        let mut markdown = String::new();
        for (number, line) in text.lines().enumerate() {
            let cells: Vec<&str> = line.split(',').collect();
            markdown.push_str(&format!("| {} |\n", cells.join(" | ")));
            if number == 0 {
                markdown.push_str(&format!("|{}\n", " --- |".repeat(cells.len())));
            }
        }
        // Return Markdown: foliq finds the headings, tables and pages in it.
        Ok(Parsed::markdown(markdown))
    }
}

fn main() -> Result<()> {
    let folder = std::env::args().nth(1).unwrap_or_else(|| ".".to_string());
    let index = Index::builder(".foliq").parser(Arc::new(CsvParser)).open()?;
    index.add_folder(folder.as_ref(), None, Vec::new())?;
    let report = index.sync()?;
    println!("{} added, {} updated", report.added, report.updated);
    for result in index.search("price", &SearchOptions::default())? {
        println!("{}", result.citation);
    }
    Ok(())
}
