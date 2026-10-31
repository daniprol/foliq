//! A parsed document: one canonical text, and its structure as positions in it.
//!
//! Every format (PDF, DOCX, HTML...) becomes Markdown-like text. Headings,
//! tables and page breaks are [`Element`]s that point into that text with byte
//! positions (`start..end`, like Python slicing `text[start:end]`, but counted
//! in bytes). Chunks are positions too, so a search hit always knows its
//! heading path, its page and its lines.

use serde::{Deserialize, Serialize};

use crate::Metadata;

/// One document, ready to be split and stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// Stable id, derived from the collection and the path.
    pub id: String,
    /// The collection the document belongs to (a named group of sources).
    pub collection: String,
    /// The path inside its collection, with `/` separators: `contracts/acme.pdf`.
    pub path: String,
    /// Where the bytes came from: `file:///home/me/docs/contracts/acme.pdf`.
    pub uri: String,
    /// A human title (from the file, the first heading, or the file name).
    pub title: String,
    /// The canonical text, Markdown-like. Every position in foliq points into it.
    pub text: String,
    /// The structure: headings, tables, page breaks... in text order.
    pub elements: Vec<Element>,
    /// Extra information from the parser or the source.
    pub metadata: Metadata,
    /// Who may see this document (`"group:hr"`, `"user:ana"`). Empty means everyone.
    pub access: Vec<String>,
    /// The source's version of the file (a modified time, an ETag...).
    pub version: String,
    /// A hash of the file's bytes: if it doesn't change, nothing is re-parsed.
    pub content_hash: String,
}

/// A piece of structure, as a byte range of [`Document::text`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Element {
    pub kind: ElementKind,
    pub start: usize,
    pub end: usize,
}

/// The kinds of structure foliq understands.
///
/// `#[serde(tag = "type")]` writes each variant as JSON like
/// `{"type": "heading", "level": 2, "title": "Refunds"}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ElementKind {
    /// A heading. `level` 1 is the biggest (`#`), 6 the smallest (`######`).
    Heading {
        level: u8,
        title: String,
    },
    Paragraph,
    ListItem,
    Table,
    Code,
    Quote,
    /// The start of a page (PDF) or a slide (PPTX). Pages count from 1.
    PageBreak {
        page: u32,
    },
    /// YAML front matter at the top of a Markdown file. Not split into chunks.
    FrontMatter,
}

/// A heading and everything under it, computed from the heading elements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Section {
    /// Its number in the outline: `"2"`, `"2.3"`... `"0"` is the text before the first heading.
    pub number: String,
    pub title: String,
    /// The heading level (1-6), or 0 for the text before the first heading.
    pub level: u8,
    /// The titles from the top heading down to this one: `["Billing", "Refunds"]`.
    pub heading_path: Vec<String>,
    /// Where the heading starts.
    pub start: usize,
    /// Where the section ends, subsections included (the next heading of the same or a higher level).
    pub end: usize,
    /// Where the section's own text ends (the next heading of any level).
    pub own_end: usize,
}

impl Document {
    /// The sections of the document, in reading order, built from its headings.
    pub fn sections(&self) -> Vec<Section> {
        let headings: Vec<(usize, u8, &str)> = self
            .elements
            .iter()
            .filter_map(|element| match &element.kind {
                ElementKind::Heading { level, title } => Some((element.start, *level, title.as_str())),
                _ => None,
            })
            .collect();

        let mut sections = Vec::new();

        // Text before the first heading becomes section "0", if it has real content.
        let first_heading_start = headings.first().map_or(self.text.len(), |heading| heading.0);
        if self.has_content(0, first_heading_start) {
            sections.push(Section {
                number: "0".to_string(),
                title: self.title.clone(),
                level: 0,
                heading_path: Vec::new(),
                start: 0,
                end: first_heading_start,
                own_end: first_heading_start,
            });
        }

        // `open_headings` holds the headings above the current one: (level, title).
        // `counters[depth]` is the running number at each depth, for "2.3"-style numbers.
        let mut open_headings: Vec<(u8, String)> = Vec::new();
        let mut counters: Vec<usize> = Vec::new();

        for (index, &(start, level, title)) in headings.iter().enumerate() {
            // Close the headings that are not above this one.
            while open_headings.last().is_some_and(|(open_level, _)| *open_level >= level) {
                open_headings.pop();
            }
            let depth = open_headings.len();
            counters.truncate(depth + 1);
            if counters.len() == depth {
                counters.push(0);
            }
            counters[depth] += 1;
            open_headings.push((level, title.to_string()));

            let later_headings = &headings[index + 1..];
            let own_end = later_headings.first().map_or(self.text.len(), |next| next.0);
            let end = later_headings
                .iter()
                .find(|next| next.1 <= level)
                .map_or(self.text.len(), |next| next.0);

            sections.push(Section {
                number: counters
                    .iter()
                    .map(|count| count.to_string())
                    .collect::<Vec<_>>()
                    .join("."),
                title: title.to_string(),
                level,
                heading_path: open_headings.iter().map(|(_, title)| title.clone()).collect(),
                start,
                end,
                own_end,
            });
        }
        sections
    }

    /// The section with this number (`"2.3"`), if there is one.
    pub fn section(&self, number: &str) -> Option<Section> {
        self.sections().into_iter().find(|section| section.number == number)
    }

    /// The innermost section that contains the byte position `offset`.
    pub fn section_at(&self, offset: usize) -> Option<Section> {
        self.sections()
            .into_iter()
            .rev() // the innermost section is the last one that contains the offset
            .find(|section| section.start <= offset && offset < section.own_end.max(section.start + 1))
    }

    /// The page (or slide) at the byte position `offset`, for formats with pages.
    pub fn page_at(&self, offset: usize) -> Option<u32> {
        self.elements
            .iter()
            .take_while(|element| element.start <= offset)
            .filter_map(|element| match element.kind {
                ElementKind::PageBreak { page } => Some(page),
                _ => None,
            })
            .last()
    }

    /// The line number (counting from 1) at the byte position `offset`.
    pub fn line_at(&self, offset: usize) -> usize {
        let offset = offset.min(self.text.len());
        self.text.as_bytes()[..offset]
            .iter()
            .filter(|&&byte| byte == b'\n')
            .count()
            + 1
    }

    /// Whether someone with these principals may see the document.
    ///
    /// `None` means "don't check" (a trusted caller, like the local CLI).
    /// A document with an empty `access` list is visible to everyone.
    pub fn is_visible_to(&self, principals: Option<&[String]>) -> bool {
        match principals {
            None => true,
            Some(principals) => {
                self.access.is_empty() || self.access.iter().any(|allowed| principals.contains(allowed))
            }
        }
    }

    /// Whether `text[start..end]` has something besides whitespace, front matter and page markers.
    fn has_content(&self, start: usize, end: usize) -> bool {
        let skipped: Vec<&Element> = self
            .elements
            .iter()
            .filter(|element| {
                matches!(element.kind, ElementKind::FrontMatter | ElementKind::PageBreak { .. })
                    && element.start < end
                    && element.end > start
            })
            .collect();
        let mut position = start;
        for element in skipped {
            if !self.text[position..element.start.max(position)].trim().is_empty() {
                return true;
            }
            position = element.end.max(position);
        }
        !self.text[position.min(end)..end].trim().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document_with_headings(text: &str, headings: &[(&str, u8)]) -> Document {
        let elements = headings
            .iter()
            .map(|(title, level)| {
                let line = format!("{} {}", "#".repeat(*level as usize), title);
                let start = text.find(&line).expect("heading is in the text");
                Element {
                    kind: ElementKind::Heading {
                        level: *level,
                        title: title.to_string(),
                    },
                    start,
                    end: start + line.len(),
                }
            })
            .collect();
        Document {
            id: "id".into(),
            collection: "docs".into(),
            path: "a.md".into(),
            uri: "file:///a.md".into(),
            title: "A".into(),
            text: text.into(),
            elements,
            metadata: Metadata::new(),
            access: Vec::new(),
            version: "1".into(),
            content_hash: "hash".into(),
        }
    }

    #[test]
    fn sections_are_numbered_like_an_outline() {
        let text = "intro\n# One\na\n## One.A\nb\n## One.B\nc\n# Two\nd\n";
        let document = document_with_headings(text, &[("One", 1), ("One.A", 2), ("One.B", 2), ("Two", 1)]);
        let sections = document.sections();
        let numbers: Vec<&str> = sections.iter().map(|section| section.number.as_str()).collect();
        assert_eq!(numbers, ["0", "1", "1.1", "1.2", "2"]);
        assert_eq!(sections[2].heading_path, ["One", "One.A"]);
        // Section "1" contains its subsections; its own text stops at "One.A".
        assert_eq!(
            &text[sections[1].start..sections[1].end],
            "# One\na\n## One.A\nb\n## One.B\nc\n"
        );
        assert_eq!(&text[sections[1].start..sections[1].own_end], "# One\na\n");
    }

    #[test]
    fn skipped_heading_levels_still_nest() {
        let text = "# Top\n### Deep\nx\n";
        let document = document_with_headings(text, &[("Top", 1), ("Deep", 3)]);
        let numbers: Vec<String> = document.sections().into_iter().map(|section| section.number).collect();
        assert_eq!(numbers, ["1", "1.1"]);
    }

    #[test]
    fn access_rules() {
        let mut document = document_with_headings("x", &[]);
        assert!(document.is_visible_to(Some(&["user:ana".to_string()])));
        document.access = vec!["group:hr".into()];
        assert!(!document.is_visible_to(Some(&["user:ana".to_string()])));
        assert!(document.is_visible_to(Some(&["user:ana".to_string(), "group:hr".to_string()])));
        assert!(document.is_visible_to(None));
    }
}
