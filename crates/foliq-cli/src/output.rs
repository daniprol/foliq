//! How each command prints its result: readable text, or JSON with `--json`.

use std::path::Path;

use foliq::{CollectionStatus, Context, DocumentInfo, Outline, Passage, SearchResult, SyncReport};
use serde::Serialize;

/// The longest search snippet, in characters.
const SNIPPET_CHARACTERS: usize = 240;

pub struct Printer {
    pub json: bool,
}

impl Printer {
    fn print_json(&self, value: &impl Serialize) {
        println!(
            "{}",
            serde_json::to_string_pretty(value).expect("results serialize to JSON")
        );
    }

    pub fn message(&self, text: &str) {
        if self.json {
            self.print_json(&serde_json::json!({ "message": text }));
        } else {
            println!("{text}");
        }
    }

    pub fn sync_report(&self, report: &SyncReport, status: &[CollectionStatus]) {
        if self.json {
            self.print_json(&serde_json::json!({ "sync": report, "collections": status }));
            return;
        }
        println!(
            "synced: {} added, {} updated, {} unchanged, {} removed",
            report.added, report.updated, report.unchanged, report.removed
        );
        for failure in &report.failed {
            let place = if failure.path.is_empty() {
                String::new()
            } else {
                format!("/{}", failure.path)
            };
            eprintln!("  failed {}{place}: {}", failure.collection, failure.message);
        }
        self.status_lines(status);
    }

    pub fn status(&self, status: &[CollectionStatus], folder: &Path) {
        if self.json {
            self.print_json(&serde_json::json!({ "index": folder, "collections": status }));
            return;
        }
        println!("index: {}", folder.display());
        if status.is_empty() {
            println!("no collections yet: run `foliq index <folder>`");
        }
        self.status_lines(status);
    }

    fn status_lines(&self, status: &[CollectionStatus]) {
        for item in status {
            let access = if item.collection.access.is_empty() {
                String::new()
            } else {
                format!("  access: {}", item.collection.access.join(", "))
            };
            println!(
                "  {}  {} documents, {} chunks, {} tokens  ({}){access}",
                item.collection.name, item.documents, item.chunks, item.tokens, item.collection.location
            );
        }
    }

    pub fn documents(&self, documents: &[DocumentInfo]) {
        if self.json {
            self.print_json(&documents);
            return;
        }
        for document in documents {
            println!(
                "{}/{}  {}  ({} tokens)",
                document.collection, document.path, document.title, document.token_count
            );
        }
    }

    pub fn outline(&self, outline: &Outline) {
        if self.json {
            self.print_json(outline);
            return;
        }
        let pages = outline.pages.map_or(String::new(), |pages| format!(", {pages} pages"));
        println!(
            "{}/{}  {}  ({} tokens{pages})",
            outline.collection, outline.path, outline.title, outline.token_count
        );
        for section in &outline.sections {
            let indent = "  ".repeat(usize::from(section.level.max(1)));
            let page = section.page.map_or(String::new(), |page| format!(", p{page}"));
            println!(
                "{indent}{}  {}  ({} tokens, L{}{page})",
                section.number, section.title, section.token_count, section.line
            );
        }
    }

    pub fn passage(&self, passage: &Passage) {
        if self.json {
            self.print_json(passage);
            return;
        }
        println!("{}", passage.text);
        if passage.truncated {
            println!(
                "\n[cut at {} tokens; see `foliq outline {}/{}` to read one section]",
                passage.token_count, passage.collection, passage.path
            );
        }
    }

    pub fn search_results(&self, results: &[SearchResult]) {
        if self.json {
            self.print_json(&results);
            return;
        }
        if results.is_empty() {
            println!("no results");
        }
        for result in results {
            let headings = result.heading_path.join(" > ");
            println!("{}  {}  [{:.2}]", result.citation, headings, result.score);
            println!("    {}", snippet(&result.text));
        }
    }

    pub fn context(&self, context: &Context) {
        if self.json {
            self.print_json(context);
        } else if context.passages.is_empty() {
            println!("no results");
        } else {
            println!("{}", context.text);
        }
    }
}

/// The chunk's text on one line, without heading lines, shortened.
fn snippet(text: &str) -> String {
    let body: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with("<!--"))
        .collect();
    let joined = body.join(" ");
    if joined.chars().count() <= SNIPPET_CHARACTERS {
        joined
    } else {
        let cut: String = joined.chars().take(SNIPPET_CHARACTERS).collect();
        format!("{}...", cut.trim_end())
    }
}
