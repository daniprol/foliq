//! Word documents (DOCX).
//!
//! The text lives in `word/document.xml` as paragraphs (`<w:p>`) and tables
//! (`<w:tbl>`). A paragraph is a heading when its style is a heading style
//! (`Heading1`, or a style named "heading 1" in `word/styles.xml`), and a list
//! item when it has numbering (`<w:numPr>`) or a list style (`ListBullet`).

use std::collections::HashMap;

use foliq_core::{Error, Parsed, Parser, Result, SourceEntry};

use super::office_xml::{OfficeFile, XmlNode};
use crate::markdown::MarkdownWriter;

pub struct DocxParser;

impl Parser for DocxParser {
    fn name(&self) -> &str {
        "docx"
    }

    fn extensions(&self) -> Vec<String> {
        super::extensions(&["docx"])
    }

    fn parse(&self, bytes: &[u8], entry: &SourceEntry) -> Result<Parsed> {
        docx_to_markdown(bytes).map_err(|message| Error::parse(&entry.path, message))
    }
}

fn docx_to_markdown(bytes: &[u8]) -> std::result::Result<Parsed, String> {
    let mut file = OfficeFile::open(bytes)?;
    let document = file.xml("word/document.xml")?.ok_or("word/document.xml is missing")?;
    let heading_styles = file
        .xml("word/styles.xml")?
        .map(|styles| heading_levels_by_style(&styles))
        .unwrap_or_default();
    let title = file.title();

    let body = document.child("body").ok_or("the document has no body")?;
    let mut writer = MarkdownWriter::new();
    write_blocks(body, &heading_styles, &mut writer);
    Ok(Parsed {
        text: writer.finish(),
        title,
        ..Parsed::default()
    })
}

/// Writes the paragraphs and tables inside `container` (the body, or a content control).
fn write_blocks(container: &XmlNode, heading_styles: &HashMap<String, u8>, writer: &mut MarkdownWriter) {
    for node in container.nodes() {
        match node.name.as_str() {
            "p" => write_paragraph(node, heading_styles, writer),
            "tbl" => writer.table(&table_rows(node)),
            // Content controls wrap ordinary paragraphs in `<w:sdt><w:sdtContent>`.
            "sdt" => {
                if let Some(content) = node.child("sdtContent") {
                    write_blocks(content, heading_styles, writer);
                }
            }
            _ => {}
        }
    }
}

fn write_paragraph(paragraph: &XmlNode, heading_styles: &HashMap<String, u8>, writer: &mut MarkdownWriter) {
    let text = paragraph.office_text();
    if text.trim().is_empty() {
        return;
    }
    let properties = paragraph.child("pPr");
    let style = properties
        .and_then(|properties| properties.find(&["pStyle"]))
        .and_then(|style| style.attribute("w:val"));

    // An outline level set on the paragraph itself also makes it a heading (0 = level 1).
    let outline_level = properties
        .and_then(|properties| properties.find(&["outlineLvl"]))
        .and_then(|level| level.attribute("w:val"))
        .and_then(|level| level.parse::<u8>().ok())
        .filter(|level| *level < 9)
        .map(|level| level + 1);

    if let Some(level) = style
        .and_then(|style| heading_styles.get(style).copied())
        .or(outline_level)
    {
        writer.heading(level, &text);
    } else if let Some(numbering) = properties.and_then(|properties| properties.child("numPr")) {
        let depth = numbering
            .find(&["ilvl"])
            .and_then(|level| level.attribute("w:val"))
            .and_then(|v| v.parse().ok());
        writer.list_item(&text, depth.unwrap_or(0));
    } else if style.is_some_and(|style| style.starts_with("List")) {
        // Styles like `ListBullet` and `ListNumber` give numbering through the style.
        writer.list_item(&text, 0);
    } else {
        writer.paragraph(&text);
    }
}

fn table_rows(table: &XmlNode) -> Vec<Vec<String>> {
    table
        .nodes()
        .filter(|node| node.name == "tr")
        .map(|row| {
            row.nodes()
                .filter(|node| node.name == "tc")
                .map(|cell| {
                    let paragraphs: Vec<String> = cell.descendants("p").iter().map(|p| p.office_text()).collect();
                    paragraphs.join(" ")
                })
                .collect()
        })
        .collect()
}

/// Which styles are headings, and at which level: style id -> level (1 = biggest).
///
/// Built-in heading styles have ids like `Heading1`, but Word in other
/// languages uses other ids (`Titre1`), so the style's name ("heading 1") and
/// its outline level are checked too.
fn heading_levels_by_style(styles: &XmlNode) -> HashMap<String, u8> {
    let mut levels = HashMap::new();
    for style in styles.nodes().filter(|node| node.name == "style") {
        let Some(id) = style.attribute("w:styleId") else {
            continue;
        };
        let name = style
            .find(&["name"])
            .and_then(|name| name.attribute("w:val"))
            .unwrap_or("")
            .to_lowercase();
        let outline_level = style
            .find(&["pPr", "outlineLvl"])
            .and_then(|level| level.attribute("w:val"))
            .and_then(|level| level.parse::<u8>().ok());

        let level = if name == "title" || id == "Title" {
            Some(1)
        } else if let Some(number) = name.strip_prefix("heading ").or_else(|| id.strip_prefix("Heading")) {
            number.trim().parse::<u8>().ok().map(|level| level.min(6))
        } else {
            outline_level.filter(|level| *level < 9).map(|level| (level + 1).min(6))
        };
        if let Some(level) = level {
            levels.insert(id.to_string(), level);
        }
    }
    // Documents without styles.xml still use the default ids.
    for level in 1..=6u8 {
        levels.entry(format!("Heading{level}")).or_insert(level);
    }
    levels
}
