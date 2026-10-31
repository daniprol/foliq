//! Markdown: foliq's canonical text format.
//!
//! - [`read_structure`] finds the elements (headings, tables, page markers...)
//!   in any Markdown text. Every parser's output goes through it.
//! - [`MarkdownWriter`] helps parsers of other formats (DOCX, HTML...) write
//!   clean Markdown.

use foliq_core::{Element, ElementKind, Metadata};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser as MarkdownParser, Tag, TagEnd};

/// The structure of a Markdown text: its elements, title and front matter.
pub struct Structure {
    pub elements: Vec<Element>,
    /// The front matter's `title`, or else the first level-1 heading.
    pub title: Option<String>,
    /// The `key: value` pairs of the front matter.
    pub front_matter: Metadata,
}

/// Finds the elements of a Markdown text.
pub fn read_structure(text: &str) -> Structure {
    let options = Options::ENABLE_TABLES | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
    let mut elements = Vec::new();
    let mut front_matter = Metadata::new();

    // The heading being read: (level, start, its title so far).
    let mut open_heading: Option<(u8, usize, String)> = None;
    // How deep we are inside lists, quotes and tables: paragraphs in there
    // belong to that bigger element and are not recorded on their own.
    let mut nesting = 0;
    let mut in_front_matter = false;

    for (event, range) in MarkdownParser::new_ext(text, options).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                open_heading = Some((heading_level_number(level), range.start, String::new()));
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, start, title)) = open_heading.take() {
                    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
                    elements.push(Element {
                        kind: ElementKind::Heading { level, title },
                        start,
                        end: range.end,
                    });
                }
            }
            Event::Text(piece) | Event::Code(piece) if open_heading.is_some() => {
                if let Some((_, _, title)) = open_heading.as_mut() {
                    title.push_str(&piece);
                }
            }
            Event::Text(piece) if in_front_matter => read_front_matter(&piece, &mut front_matter),
            Event::Start(Tag::MetadataBlock(_)) => {
                in_front_matter = true;
                elements.push(Element {
                    kind: ElementKind::FrontMatter,
                    start: range.start,
                    end: range.end,
                });
            }
            Event::End(TagEnd::MetadataBlock(_)) => in_front_matter = false,
            Event::Start(Tag::Paragraph) if nesting == 0 => {
                elements.push(Element {
                    kind: ElementKind::Paragraph,
                    start: range.start,
                    end: range.end,
                });
            }
            Event::Start(Tag::Item) => {
                elements.push(Element {
                    kind: ElementKind::ListItem,
                    start: range.start,
                    end: range.end,
                });
            }
            Event::Start(Tag::Table(_)) => {
                nesting += 1;
                elements.push(Element {
                    kind: ElementKind::Table,
                    start: range.start,
                    end: range.end,
                });
            }
            Event::Start(Tag::BlockQuote(_)) => {
                nesting += 1;
                elements.push(Element {
                    kind: ElementKind::Quote,
                    start: range.start,
                    end: range.end,
                });
            }
            Event::Start(Tag::List(_)) => nesting += 1,
            Event::End(TagEnd::Table | TagEnd::BlockQuote(_) | TagEnd::List(_)) => nesting -= 1,
            Event::Start(Tag::CodeBlock(_)) => {
                elements.push(Element {
                    kind: ElementKind::Code,
                    start: range.start,
                    end: range.end,
                });
            }
            Event::Html(html) => {
                if let Some(page) = page_marker_number(&html) {
                    elements.push(Element {
                        kind: ElementKind::PageBreak { page },
                        start: range.start,
                        end: range.end,
                    });
                }
            }
            _ => {}
        }
    }

    let title = front_matter
        .get("title")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .or_else(|| {
            elements.iter().find_map(|element| match &element.kind {
                ElementKind::Heading { level: 1, title } if !title.is_empty() => Some(title.clone()),
                _ => None,
            })
        });

    Structure {
        elements,
        title,
        front_matter,
    }
}

/// The page number in a marker line like `<!-- page 3 -->` or `<!-- slide 3 -->`.
pub fn page_marker_number(html: &str) -> Option<u32> {
    let inside = html.trim().strip_prefix("<!--")?.strip_suffix("-->")?;
    let mut words = inside.split_whitespace();
    let kind = words.next()?;
    if kind != "page" && kind != "slide" {
        return None;
    }
    words.next()?.parse().ok()
}

/// Reads simple `key: value` lines of YAML front matter (enough for titles, tags, dates).
fn read_front_matter(text: &str, front_matter: &mut Metadata) {
    for line in text.lines() {
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim();
            let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
            if !key.is_empty() && !key.starts_with(' ') && !value.is_empty() {
                front_matter.insert(key.to_string(), value.into());
            }
        }
    }
}

fn heading_level_number(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Builds clean Markdown, one block at a time. Used by the parsers of other formats.
#[derive(Default)]
pub struct MarkdownWriter {
    text: String,
}

impl MarkdownWriter {
    pub fn new() -> MarkdownWriter {
        MarkdownWriter::default()
    }

    /// `## Title`. Levels outside 1-6 are clamped.
    pub fn heading(&mut self, level: u8, title: &str) {
        let title = one_line(title);
        if !title.is_empty() {
            let hashes = "#".repeat(level.clamp(1, 6) as usize);
            self.block(&format!("{hashes} {title}"));
        }
    }

    pub fn paragraph(&mut self, text: &str) {
        let text = one_line(text);
        if !text.is_empty() {
            self.block(&escape_block_start(&text));
        }
    }

    /// A bullet, indented by `depth` (0 = top level). Consecutive items form one list.
    pub fn list_item(&mut self, text: &str, depth: usize) {
        let text = one_line(text);
        if text.is_empty() {
            return;
        }
        // List items are separated by a single line break, not a blank line.
        if self.text.ends_with("\n\n") {
            let previous_line = self.text.trim_end().rsplit('\n').next().unwrap_or("");
            if previous_line.trim_start().starts_with("- ") {
                self.text.pop();
            }
        }
        self.text.push_str(&format!("{}- {}\n\n", "  ".repeat(depth), text));
    }

    /// A table. The first row is the header.
    pub fn table(&mut self, rows: &[Vec<String>]) {
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        if width == 0 {
            return;
        }
        let mut lines = Vec::with_capacity(rows.len() + 1);
        for (index, row) in rows.iter().enumerate() {
            let cells: Vec<String> = (0..width)
                .map(|column| table_cell(row.get(column).map_or("", String::as_str)))
                .collect();
            lines.push(format!("| {} |", cells.join(" | ")));
            if index == 0 {
                lines.push(format!("|{}|", vec![" --- "; width].join("|")));
            }
        }
        self.block(&lines.join("\n"));
    }

    /// A fenced code block.
    pub fn code(&mut self, code: &str) {
        if !code.trim().is_empty() {
            self.block(&format!("```\n{}\n```", code.trim_end()));
        }
    }

    /// A page (or slide) marker: `<!-- page 3 -->`.
    pub fn page(&mut self, kind: &str, number: u32) {
        self.block(&format!("<!-- {kind} {number} -->"));
    }

    /// Adds Markdown that is already well formed (another converter's output).
    pub fn raw(&mut self, markdown: &str) {
        if !markdown.trim().is_empty() {
            self.block(markdown.trim());
        }
    }

    /// The finished Markdown.
    pub fn finish(self) -> String {
        let mut text = self.text;
        text.truncate(text.trim_end().len());
        text.push('\n');
        text
    }

    fn block(&mut self, block: &str) {
        self.text.push_str(block);
        self.text.push_str("\n\n");
    }
}

/// Joins all whitespace (line breaks included) into single spaces.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A paragraph starting with `#`, `-`, `>` or `1.` would turn into another block: escape it.
fn escape_block_start(text: &str) -> String {
    let starts_like_markup = text.starts_with(['#', '-', '>', '*', '+', '|'])
        || text
            .split_once(". ")
            .is_some_and(|(number, _)| number.chars().all(|c| c.is_ascii_digit()));
    if starts_like_markup {
        format!("\\{text}")
    } else {
        text.to_string()
    }
}

fn table_cell(text: &str) -> String {
    one_line(text).replace('|', "\\|")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_headings_tables_and_pages() {
        let text = "---\ntitle: Handbook\ntags: hr\n---\n\n<!-- page 1 -->\n\n# Billing\n\nSome text.\n\n## Refunds `v2`\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n<!-- page 2 -->\n\n- one\n- two\n";
        let structure = read_structure(text);
        assert_eq!(structure.title.as_deref(), Some("Handbook"));
        assert_eq!(structure.front_matter["tags"], "hr");
        let kinds: Vec<&ElementKind> = structure.elements.iter().map(|element| &element.kind).collect();
        assert_eq!(
            kinds,
            [
                &ElementKind::FrontMatter,
                &ElementKind::PageBreak { page: 1 },
                &ElementKind::Heading {
                    level: 1,
                    title: "Billing".into()
                },
                &ElementKind::Paragraph,
                &ElementKind::Heading {
                    level: 2,
                    title: "Refunds v2".into()
                },
                &ElementKind::Table,
                &ElementKind::PageBreak { page: 2 },
                &ElementKind::ListItem,
                &ElementKind::ListItem,
            ]
        );
        let heading = &structure.elements[2];
        assert_eq!(&text[heading.start..heading.end].trim_end(), &"# Billing");
    }

    #[test]
    fn title_falls_back_to_the_first_h1() {
        assert_eq!(
            read_structure("intro\n\n# Main\n\n# Other\n").title.as_deref(),
            Some("Main")
        );
        assert_eq!(read_structure("no headings").title, None);
    }

    #[test]
    fn writer_builds_markdown_that_reads_back() {
        let mut writer = MarkdownWriter::new();
        writer.page("page", 1);
        writer.heading(1, "Report\n2026");
        writer.paragraph("# not a heading");
        writer.list_item("first", 0);
        writer.list_item("second", 0);
        writer.table(&[vec!["name".into(), "a|b".into()], vec!["x".into()]]);
        let text = writer.finish();
        assert_eq!(
            text,
            "<!-- page 1 -->\n\n# Report 2026\n\n\\# not a heading\n\n- first\n- second\n\n| name | a\\|b |\n| --- | --- |\n| x |  |\n"
        );
        let structure = read_structure(&text);
        assert_eq!(
            structure
                .elements
                .iter()
                .filter(|e| matches!(e.kind, ElementKind::Heading { .. }))
                .count(),
            1
        );
    }
}
