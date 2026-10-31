//! PowerPoint presentations (PPTX): one section per slide, with its speaker notes.
//!
//! The slide order is in `ppt/presentation.xml`, which links to each slide's
//! XML through relationships. Every slide becomes:
//!
//! ```text
//! <!-- slide 3 -->
//!
//! # The slide title
//!
//! - bullet
//!   - sub-bullet
//!
//! Notes: what the speaker says
//! ```

use foliq_core::{Error, Parsed, Parser, Result, SourceEntry};

use super::office_xml::{OfficeFile, XmlNode};
use crate::markdown::MarkdownWriter;

pub struct PptxParser;

impl Parser for PptxParser {
    fn name(&self) -> &str {
        "pptx"
    }

    fn extensions(&self) -> Vec<String> {
        super::extensions(&["pptx"])
    }

    fn parse(&self, bytes: &[u8], entry: &SourceEntry) -> Result<Parsed> {
        pptx_to_markdown(bytes).map_err(|message| Error::parse(&entry.path, message))
    }
}

fn pptx_to_markdown(bytes: &[u8]) -> std::result::Result<Parsed, String> {
    let mut file = OfficeFile::open(bytes)?;
    let presentation = file
        .xml("ppt/presentation.xml")?
        .ok_or("ppt/presentation.xml is missing")?;
    let links = file.relationships("ppt/presentation.xml")?;

    let slide_paths: Vec<String> = presentation
        .descendants("sldId")
        .iter()
        .filter_map(|slide| slide.attribute("r:id"))
        .filter_map(|id| links.get(id).map(|(path, _)| path.clone()))
        .collect();

    let mut writer = MarkdownWriter::new();
    let mut first_title = None;
    for (index, slide_path) in slide_paths.iter().enumerate() {
        let number = index as u32 + 1;
        let Some(slide) = file.xml(slide_path)? else { continue };
        writer.page("slide", number);

        let title = slide_title(&slide);
        if first_title.is_none() {
            first_title.clone_from(&title);
        }
        writer.heading(1, title.as_deref().unwrap_or(&format!("Slide {number}")));
        write_slide_body(&slide, &mut writer);

        // Speaker notes are linked from the slide's relationships.
        let notes_path = file
            .relationships(slide_path)?
            .into_values()
            .find(|(_, kind)| kind.ends_with("/notesSlide"))
            .map(|(path, _)| path);
        if let Some(notes) = notes_path.and_then(|path| file.xml(&path).ok().flatten()) {
            let text = placeholder_paragraphs(&notes, &["body"]).join(" ");
            if !text.trim().is_empty() {
                writer.paragraph(&format!("Notes: {text}"));
            }
        }
    }
    let title = file.title().or(first_title);
    Ok(Parsed {
        text: writer.finish(),
        title,
        ..Parsed::default()
    })
}

/// The kind of placeholder a shape is (`title`, `body`...), from `<p:nvSpPr><p:nvPr><p:ph type="…">`.
fn placeholder_kind(shape: &XmlNode) -> Option<&str> {
    let placeholder = shape.find(&["nvSpPr", "nvPr", "ph"])?;
    // A placeholder without a type is a body placeholder.
    Some(placeholder.attribute("type").unwrap_or("body"))
}

fn slide_title(slide: &XmlNode) -> Option<String> {
    let title = placeholder_paragraphs(slide, &["title", "ctrTitle"]).join(" ");
    let title = title.trim();
    (!title.is_empty()).then(|| title.to_string())
}

/// The text paragraphs of the shapes that are placeholders of these kinds.
fn placeholder_paragraphs(slide: &XmlNode, kinds: &[&str]) -> Vec<String> {
    slide
        .descendants("sp")
        .into_iter()
        .filter(|shape| placeholder_kind(shape).is_some_and(|kind| kinds.contains(&kind)))
        .flat_map(|shape| {
            shape
                .descendants("p")
                .into_iter()
                .map(|paragraph| paragraph.office_text())
        })
        .filter(|text| !text.trim().is_empty())
        .collect()
}

/// Every shape except the title, in order: text as bullets, tables as tables.
fn write_slide_body(slide: &XmlNode, writer: &mut MarkdownWriter) {
    let Some(shapes) = slide.find(&["cSld", "spTree"]) else {
        return;
    };
    write_shapes(shapes, writer);
}

fn write_shapes(group: &XmlNode, writer: &mut MarkdownWriter) {
    for shape in group.nodes() {
        match shape.name.as_str() {
            "sp" => {
                if matches!(
                    placeholder_kind(shape),
                    Some("title" | "ctrTitle" | "sldNum" | "dt" | "ftr")
                ) {
                    continue;
                }
                let Some(text_body) = shape.child("txBody") else {
                    continue;
                };
                for paragraph in text_body.nodes().filter(|node| node.name == "p") {
                    let depth = paragraph
                        .find(&["pPr"])
                        .and_then(|properties| properties.attribute("lvl"))
                        .and_then(|level| level.parse().ok())
                        .unwrap_or(0);
                    writer.list_item(&paragraph.office_text(), depth);
                }
            }
            "graphicFrame" => {
                for table in shape.descendants("tbl") {
                    let rows: Vec<Vec<String>> = table
                        .nodes()
                        .filter(|node| node.name == "tr")
                        .map(|row| {
                            row.nodes()
                                .filter(|node| node.name == "tc")
                                .map(|cell| cell.office_text())
                                .collect()
                        })
                        .collect();
                    writer.table(&rows);
                }
            }
            // Grouped shapes.
            "grpSp" => write_shapes(shape, writer),
            _ => {}
        }
    }
}
