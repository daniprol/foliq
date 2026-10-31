//! The default splitter: one chunk per section, smaller pieces only when a
//! section is too big.
//!
//! A chunk never crosses a heading, so every chunk has exactly one heading
//! path. Big sections are cut with the `text-splitter` crate, which prefers
//! Markdown boundaries (paragraphs, list items, table rows, sentences) and
//! never splits inside a code block when it can avoid it.

use foliq_core::{Chunk, Document, ElementKind, Result, Section, Splitter};
use text_splitter::{ChunkConfig, MarkdownSplitter};

use crate::markdown::page_marker_number;
use crate::tokens::{count_tokens, tokenizer};

/// Splits documents along their headings.
pub struct StructureSplitter {
    /// The biggest chunk, in tokens.
    pub max_tokens: usize,
}

impl Default for StructureSplitter {
    fn default() -> Self {
        StructureSplitter { max_tokens: 400 }
    }
}

impl Splitter for StructureSplitter {
    fn name(&self) -> &str {
        "structure"
    }

    fn fingerprint(&self) -> String {
        format!("structure@1:max_tokens={}", self.max_tokens)
    }

    fn split(&self, document: &Document) -> Result<Vec<Chunk>> {
        let mut pieces = Vec::new();
        for section in document.sections() {
            let start = skip_front_matter(document, section.start);
            let end = section.own_end;
            if start >= end || !has_body(document, start, end) {
                continue;
            }
            let section_text = &document.text[start..end];
            if count_tokens(section_text) <= self.max_tokens {
                pieces.push((start, end, section.clone()));
            } else {
                let config = ChunkConfig::new(self.max_tokens).with_sizer(tokenizer());
                for (offset, piece) in MarkdownSplitter::new(config).chunk_indices(section_text) {
                    pieces.push((start + offset, start + offset + piece.len(), section.clone()));
                }
            }
        }

        let mut chunks = Vec::new();
        for (start, end, section) in pieces {
            let (start, end) = trim_range(&document.text, start, end);
            let text = &document.text[start..end];
            if is_only_markup(text) {
                continue;
            }
            chunks.push(make_chunk(document, chunks.len(), start, end, &section));
        }
        Ok(chunks)
    }
}

/// Builds the chunk for `document.text[start..end]`, in `section`.
///
/// Every splitter can use it, so all chunks carry the same positions:
/// lines, pages, section number and heading path.
pub fn make_chunk(document: &Document, ordinal: usize, start: usize, end: usize, section: &Section) -> Chunk {
    let text = &document.text[start..end];
    let last = end.saturating_sub(1).max(start);
    Chunk {
        id: format!("{}:{ordinal}", document.id),
        document_id: document.id.clone(),
        ordinal,
        start,
        end,
        line_start: document.line_at(start),
        line_end: document.line_at(last),
        page_start: document.page_at(start),
        page_end: document.page_at(last),
        section: section.number.clone(),
        heading_path: section.heading_path.clone(),
        token_count: count_tokens(text),
        text: text.to_string(),
        content_key: blake3::hash(text.as_bytes()).to_hex()[..32].to_string(),
    }
}

/// Moves `start` past front matter that begins there.
fn skip_front_matter(document: &Document, start: usize) -> usize {
    document
        .elements
        .iter()
        .find(|element| element.kind == ElementKind::FrontMatter && element.start <= start && start < element.end)
        .map_or(start, |front_matter| front_matter.end)
}

/// Whether the range has text besides its own heading line.
fn has_body(document: &Document, start: usize, end: usize) -> bool {
    let heading_end = document
        .elements
        .iter()
        .find(|element| matches!(element.kind, ElementKind::Heading { .. }) && element.start == start)
        .map_or(start, |heading| heading.end);
    !is_only_markup(&document.text[heading_end.min(end)..end])
}

/// Whether the text is empty once page markers and whitespace are removed.
fn is_only_markup(text: &str) -> bool {
    text.lines()
        .all(|line| line.trim().is_empty() || page_marker_number(line).is_some())
}

/// Shrinks a byte range so it doesn't start or end with whitespace.
fn trim_range(text: &str, start: usize, end: usize) -> (usize, usize) {
    let piece = &text[start..end];
    let leading = piece.len() - piece.trim_start().len();
    let trailing = piece.len() - piece.trim_end().len();
    (start + leading, (end - trailing).max(start + leading))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::read_structure;

    fn document(text: &str) -> Document {
        let structure = read_structure(text);
        Document {
            id: "doc".into(),
            collection: "docs".into(),
            path: "a.md".into(),
            uri: "file:///a.md".into(),
            title: structure.title.unwrap_or_default(),
            text: text.into(),
            elements: structure.elements,
            metadata: Default::default(),
            access: Vec::new(),
            version: "1".into(),
            content_hash: "h".into(),
        }
    }

    #[test]
    fn one_chunk_per_section_with_its_heading_path() {
        let text = "---\ntitle: T\n---\n\nIntro text.\n\n# Billing\n\n## Refunds\n\nRefunds take 5 days.\n\n# Support\n\nEmail us.\n";
        let chunks = StructureSplitter::default().split(&document(text)).unwrap();
        let summary: Vec<(&str, Vec<String>, &str)> = chunks
            .iter()
            .map(|chunk| (chunk.section.as_str(), chunk.heading_path.clone(), chunk.text.as_str()))
            .collect();
        assert_eq!(
            summary,
            [
                ("0", vec![], "Intro text."),
                (
                    "1.1",
                    vec!["Billing".to_string(), "Refunds".to_string()],
                    "## Refunds\n\nRefunds take 5 days."
                ),
                ("2", vec!["Support".to_string()], "# Support\n\nEmail us."),
            ]
        );
        // Positions point back into the document.
        assert_eq!(&text[chunks[1].start..chunks[1].end], chunks[1].text);
        assert_eq!(chunks[1].line_start, 9);
        assert_eq!(chunks[1].line_end, 11);
    }

    #[test]
    fn big_sections_are_cut_and_pages_are_tracked() {
        let mut text = String::from("<!-- page 1 -->\n\n# Long\n\n");
        for number in 0..60 {
            if number == 30 {
                text.push_str("<!-- page 2 -->\n\n");
            }
            text.push_str(&format!("Paragraph {number} has a few words in it to fill space.\n\n"));
        }
        let chunks = StructureSplitter { max_tokens: 100 }.split(&document(&text)).unwrap();
        assert!(chunks.len() > 3);
        assert!(chunks.iter().all(|chunk| chunk.token_count <= 100));
        assert!(chunks.iter().all(|chunk| chunk.heading_path == ["Long"]));
        assert_eq!(chunks.first().unwrap().page_start, Some(1));
        assert_eq!(chunks.last().unwrap().page_end, Some(2));
    }
}
