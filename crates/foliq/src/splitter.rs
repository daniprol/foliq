//! The default splitter: one chunk per section, smaller pieces only when a
//! section is too big.
//!
//! A chunk never crosses a heading, so every chunk has exactly one heading
//! path. Big sections are cut with the `text-splitter` crate, which prefers
//! Markdown boundaries (paragraphs, list items, table rows, sentences) and
//! never splits inside a code block when it can avoid it.

use std::collections::HashMap;

use foliq_core::{Chunk, Document, Element, ElementKind, Positions, Result, Section, Splitter};
use text_splitter::{ChunkConfig, MarkdownSplitter};

use crate::markdown::page_marker_number;
use crate::tokens::{TokenCounter, count_tokens};

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
        format!("structure@2:max_tokens={}", self.max_tokens)
    }

    fn split(&self, document: &Document) -> Result<Vec<Chunk>> {
        // Built once: looking these up per section would be quadratic in big documents.
        let heading_ends: HashMap<usize, usize> = document
            .elements
            .iter()
            .filter(|element| matches!(element.kind, ElementKind::Heading { .. }))
            .map(|heading| (heading.start, heading.end))
            .collect();
        let front_matter: Vec<&Element> = document
            .elements
            .iter()
            .filter(|element| element.kind == ElementKind::FrontMatter)
            .collect();

        let mut pieces = Vec::new();
        for section in document.sections() {
            let start = skip_front_matter(&front_matter, section.start);
            let end = section.own_end;
            if start >= end || !has_body(document, &heading_ends, start, end) {
                continue;
            }
            // Each piece keeps its token count when it is already known, so
            // the text is not tokenized twice.
            let (start, end) = trim_range(&document.text, start, end);
            let section_text = &document.text[start..end];
            let tokens = count_tokens(section_text);
            if tokens <= self.max_tokens {
                pieces.push((start, end, section.clone(), Some(tokens)));
            } else if is_mostly_rows(section_text) {
                // Big tables and long lists (spreadsheets, long HTML tables) are
                // cut between rows, in one pass.
                for (offset, length, tokens) in pack_lines(section_text, self.max_tokens) {
                    pieces.push((start + offset, start + offset + length, section.clone(), tokens));
                }
            } else {
                let config = ChunkConfig::new(self.max_tokens).with_sizer(TokenCounter);
                for (offset, piece) in MarkdownSplitter::new(config).chunk_indices(section_text) {
                    pieces.push((start + offset, start + offset + piece.len(), section.clone(), None));
                }
            }
        }

        let positions = document.positions();
        let mut chunks = Vec::new();
        for (start, end, section, tokens) in pieces {
            let (start, end) = trim_range(&document.text, start, end);
            let text = &document.text[start..end];
            if is_only_markup(text) {
                continue;
            }
            chunks.push(make_chunk(
                document,
                &positions,
                chunks.len(),
                start,
                end,
                &section,
                tokens,
            ));
        }
        Ok(chunks)
    }
}

/// Builds the chunk for `document.text[start..end]`, in `section`.
///
/// Every splitter can use it, so all chunks carry the same positions:
/// lines, pages, section number and heading path.
/// `positions` is `document.positions()`, built once for all the chunks of the
/// document. `token_count` is the chunk's size if the splitter already
/// measured it, or `None` to count it here.
pub fn make_chunk(
    document: &Document,
    positions: &Positions,
    ordinal: usize,
    start: usize,
    end: usize,
    section: &Section,
    token_count: Option<usize>,
) -> Chunk {
    let text = &document.text[start..end];
    let last = end.saturating_sub(1).max(start);
    Chunk {
        id: format!("{}:{ordinal}", document.id),
        document_id: document.id.clone(),
        ordinal,
        start,
        end,
        line_start: positions.line_at(start),
        line_end: positions.line_at(last),
        page_start: positions.page_at(start),
        page_end: positions.page_at(last),
        section: section.number.clone(),
        heading_path: section.heading_path.clone(),
        token_count: token_count.unwrap_or_else(|| count_tokens(text)),
        text: text.to_string(),
        content_key: blake3::hash(text.as_bytes()).to_hex()[..32].to_string(),
    }
}

/// Whether most lines of the text are table rows (`| a | b |`) or list items (`- a`).
fn is_mostly_rows(text: &str) -> bool {
    let mut lines = 0;
    let mut rows = 0;
    // Headings don't count: a block of rows starts with one.
    for line in text
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
    {
        lines += 1;
        let line = line.trim_start();
        if line.starts_with('|') || line.starts_with("- ") {
            rows += 1;
        }
    }
    rows * 5 >= lines * 4
}

/// Packs whole lines into pieces of at most `max_tokens`, counting each line
/// once. A line that is too long on its own is cut into equal parts.
/// Returns `(offset, length, tokens)` for each piece, in bytes; `tokens` is
/// `None` for the parts of a cut line, which are counted later.
fn pack_lines(text: &str, max_tokens: usize) -> Vec<(usize, usize, Option<usize>)> {
    let mut pieces = Vec::new();
    let mut piece_start = 0;
    let mut piece_tokens = 0;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let line_tokens = count_tokens(line);
        if piece_tokens + line_tokens > max_tokens && offset > piece_start {
            pieces.push((piece_start, offset - piece_start, Some(piece_tokens)));
            piece_start = offset;
            piece_tokens = 0;
        }
        if line_tokens > max_tokens {
            // One line bigger than a chunk: cut it into equal parts, at character boundaries.
            let parts = line_tokens.div_ceil(max_tokens);
            let mut part_start = offset;
            for part in 1..=parts {
                let mut part_end = offset + line.len() * part / parts;
                while !text.is_char_boundary(part_end) {
                    part_end += 1;
                }
                if part_end > part_start {
                    pieces.push((part_start, part_end - part_start, None));
                }
                part_start = part_end;
            }
            piece_start = offset + line.len();
        } else {
            piece_tokens += line_tokens;
        }
        offset += line.len();
    }
    if offset > piece_start {
        pieces.push((piece_start, offset - piece_start, Some(piece_tokens)));
    }
    pieces
}

/// Moves `start` past front matter that begins there.
fn skip_front_matter(front_matter: &[&Element], start: usize) -> usize {
    front_matter
        .iter()
        .find(|element| element.start <= start && start < element.end)
        .map_or(start, |element| element.end)
}

/// Whether the range has text besides its own heading line.
/// `heading_ends` maps where each heading starts to where it ends.
fn has_body(document: &Document, heading_ends: &HashMap<usize, usize>, start: usize, end: usize) -> bool {
    let heading_end = heading_ends.get(&start).copied().unwrap_or(start);
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

    #[test]
    fn big_tables_are_cut_between_rows() {
        let mut text = String::from("# Prices\n\n| Product | Price |\n| --- | --- |\n");
        for number in 0..300 {
            text.push_str(&format!("| Product number {number} | {number}.50 |\n"));
        }
        let chunks = StructureSplitter { max_tokens: 100 }.split(&document(&text)).unwrap();
        assert!(chunks.len() > 10);
        for chunk in &chunks {
            assert!(chunk.token_count <= 100);
            let rows = chunk.text.trim_start_matches("# Prices").trim();
            assert!(rows.starts_with('|') && rows.ends_with('|'), "{}", chunk.text);
        }
    }

    #[test]
    fn a_line_longer_than_a_chunk_is_cut() {
        let text = format!("# Hash\n\n| {} |\n", "f".repeat(20_000));
        let chunks = StructureSplitter { max_tokens: 100 }.split(&document(&text)).unwrap();
        assert!(chunks.len() > 5);
        assert!(chunks.iter().all(|chunk| chunk.token_count <= 120));
    }
}
