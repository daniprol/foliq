//! Shared helpers for Office files (DOCX, PPTX), which are zip files full of XML.
//!
//! The XML is read into a small tree of [`XmlNode`]s, much like Python's
//! `xml.etree.ElementTree`, so the parsers can walk it with plain loops.

use std::collections::HashMap;
use std::io::{Cursor, Read};

use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};
use zip::ZipArchive;

/// One XML element: its name (without the namespace prefix), attributes and children.
#[derive(Debug, Default)]
pub struct XmlNode {
    /// The local name: `p` for `<w:p>`.
    pub name: String,
    /// Attributes with their full names, like `("w:val", "Heading1")`.
    pub attributes: Vec<(String, String)>,
    pub children: Vec<XmlChild>,
}

#[derive(Debug)]
pub enum XmlChild {
    Node(XmlNode),
    Text(String),
}

impl XmlNode {
    /// The value of an attribute, by its full name (`"w:val"`).
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// The element children (text children skipped).
    pub fn nodes(&self) -> impl Iterator<Item = &XmlNode> {
        self.children.iter().filter_map(|child| match child {
            XmlChild::Node(node) => Some(node),
            XmlChild::Text(_) => None,
        })
    }

    /// The first child element with this name.
    pub fn child(&self, name: &str) -> Option<&XmlNode> {
        self.nodes().find(|node| node.name == name)
    }

    /// Follows a path of child names: `node.find(&["pPr", "pStyle"])`.
    pub fn find(&self, path: &[&str]) -> Option<&XmlNode> {
        path.iter().try_fold(self, |node, name| node.child(name))
    }

    /// Every element below this one (at any depth) with this name, in document order.
    pub fn descendants(&self, name: &str) -> Vec<&XmlNode> {
        let mut found = Vec::new();
        for node in self.nodes() {
            if node.name == name {
                found.push(node);
            }
            found.extend(node.descendants(name));
        }
        found
    }

    /// The text of every `<…:t>` element below this one, joined (how Office stores visible text).
    pub fn office_text(&self) -> String {
        let mut text = String::new();
        self.collect_office_text(&mut text);
        text
    }

    fn collect_office_text(&self, text: &mut String) {
        for child in &self.children {
            if let XmlChild::Node(node) = child {
                match node.name.as_str() {
                    "t" => text.push_str(&node.plain_text()),
                    "tab" => text.push('\t'),
                    "br" | "cr" => text.push('\n'),
                    _ => node.collect_office_text(text),
                }
            }
        }
    }

    /// All text directly or indirectly inside this element.
    fn plain_text(&self) -> String {
        self.children
            .iter()
            .map(|child| match child {
                XmlChild::Text(text) => text.clone(),
                XmlChild::Node(node) => node.plain_text(),
            })
            .collect()
    }
}

/// How deep elements may nest. Real documents stay far below this; a file
/// that goes deeper (some test files nest 15,000 tables) would otherwise
/// overflow the stack in the recursive code that walks the tree.
const MAX_DEPTH: usize = 1000;

/// Reads an XML document into a tree. Returns the root element.
pub fn parse_xml(xml: &str) -> Result<XmlNode, String> {
    let mut reader = Reader::from_str(xml);
    // The stack of open elements. The bottom one is a placeholder that collects the root.
    let mut open: Vec<XmlNode> = vec![XmlNode::default()];

    loop {
        match reader.read_event().map_err(|error| error.to_string())? {
            Event::Start(start) => {
                if open.len() > MAX_DEPTH {
                    return Err(format!("XML elements nested deeper than {MAX_DEPTH} levels"));
                }
                open.push(node_from_tag(&start)?);
            }
            Event::Empty(empty) => {
                let node = node_from_tag(&empty)?;
                open.last_mut()
                    .expect("never empty")
                    .children
                    .push(XmlChild::Node(node));
            }
            Event::End(_) => {
                let node = open.pop().expect("never empty");
                let parent = open.last_mut().ok_or("unbalanced XML")?;
                parent.children.push(XmlChild::Node(node));
            }
            Event::Text(text) => push_text(&mut open, text.xml10_content().into_owned()),
            Event::CData(data) => push_text(&mut open, data.xml10_content().into_owned()),
            Event::GeneralRef(reference) => {
                let character = match reference.resolve_char_ref().map_err(|error| error.to_string())? {
                    Some(character) => character.to_string(),
                    None => named_entity(&reference).to_string(),
                };
                push_text(&mut open, character);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let placeholder = open.into_iter().next().ok_or("unbalanced XML")?;
    placeholder
        .children
        .into_iter()
        .find_map(|child| match child {
            XmlChild::Node(node) => Some(node),
            XmlChild::Text(_) => None,
        })
        .ok_or_else(|| "empty XML document".to_string())
}

fn node_from_tag(tag: &quick_xml::events::BytesStart) -> Result<XmlNode, String> {
    let name = tag.local_name().as_ref().to_string();
    let mut attributes = Vec::new();
    for attribute in tag.attributes().with_checks(false) {
        let attribute = attribute.map_err(|error| error.to_string())?;
        let key = attribute.key.as_ref().to_string();
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|error| error.to_string())?
            .into_owned();
        attributes.push((key, value));
    }
    Ok(XmlNode {
        name,
        attributes,
        children: Vec::new(),
    })
}

fn push_text(open: &mut [XmlNode], text: String) {
    if let Some(node) = open.last_mut() {
        node.children.push(XmlChild::Text(text));
    }
}

fn named_entity(name: &str) -> &'static str {
    match name {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        _ => "",
    }
}

/// An Office file opened as a zip archive.
pub struct OfficeFile<'bytes> {
    archive: ZipArchive<Cursor<&'bytes [u8]>>,
}

impl<'bytes> OfficeFile<'bytes> {
    pub fn open(bytes: &'bytes [u8]) -> Result<OfficeFile<'bytes>, String> {
        let archive =
            ZipArchive::new(Cursor::new(bytes)).map_err(|error| format!("not a valid Office file: {error}"))?;
        Ok(OfficeFile { archive })
    }

    /// The XML part at `path` (like `word/document.xml`), or `None` if it doesn't exist.
    pub fn xml(&mut self, path: &str) -> Result<Option<XmlNode>, String> {
        let mut file = match self.archive.by_name(path) {
            Ok(file) => file,
            Err(_) => return Ok(None),
        };
        let mut xml = String::new();
        file.read_to_string(&mut xml)
            .map_err(|error| format!("{path}: {error}"))?;
        parse_xml(&xml).map(Some).map_err(|error| format!("{path}: {error}"))
    }

    /// The relationships of a part: relationship id -> (target path, type).
    ///
    /// Office files link their parts through `_rels/*.rels` files, like
    /// `rId3 -> slides/slide1.xml`. Targets are returned as full paths in the zip.
    pub fn relationships(&mut self, part_path: &str) -> Result<HashMap<String, (String, String)>, String> {
        let (folder, file_name) = part_path.rsplit_once('/').unwrap_or(("", part_path));
        let rels_path = if folder.is_empty() {
            format!("_rels/{file_name}.rels")
        } else {
            format!("{folder}/_rels/{file_name}.rels")
        };
        let mut relationships = HashMap::new();
        if let Some(rels) = self.xml(&rels_path)? {
            for relationship in rels.nodes() {
                let (Some(id), Some(target)) = (relationship.attribute("Id"), relationship.attribute("Target")) else {
                    continue;
                };
                let kind = relationship.attribute("Type").unwrap_or("").to_string();
                relationships.insert(id.to_string(), (join_part_path(folder, target), kind));
            }
        }
        Ok(relationships)
    }

    /// The title stored in the file's properties (`docProps/core.xml`), if any.
    pub fn title(&mut self) -> Option<String> {
        let core = self.xml("docProps/core.xml").ok()??;
        let title = core.child("title")?.plain_text();
        let title = title.trim();
        (!title.is_empty()).then(|| title.to_string())
    }
}

/// Resolves a relationship target (`../notesSlides/notesSlide1.xml`) against a folder (`ppt/slides`).
fn join_part_path(folder: &str, target: &str) -> String {
    if let Some(absolute) = target.strip_prefix('/') {
        return absolute.to_string();
    }
    let mut parts: Vec<&str> = folder.split('/').filter(|part| !part.is_empty()).collect();
    for piece in target.split('/') {
        match piece {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            name => parts.push(name),
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_xml_into_a_tree() {
        let xml = r#"<?xml version="1.0"?><w:document xmlns:w="x"><w:body><w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Fish &amp; chips&#33;</w:t></w:r></w:p></w:body></w:document>"#;
        let root = parse_xml(xml).unwrap();
        assert_eq!(root.name, "document");
        let paragraph = root.find(&["body", "p"]).unwrap();
        assert_eq!(
            paragraph.find(&["pPr", "pStyle"]).unwrap().attribute("w:val"),
            Some("Heading1")
        );
        assert_eq!(paragraph.office_text(), "Fish & chips!");
    }

    #[test]
    fn refuses_xml_nested_too_deeply() {
        let xml = format!("{}{}", "<a>".repeat(MAX_DEPTH + 5), "</a>".repeat(MAX_DEPTH + 5));
        assert!(parse_xml(&xml).unwrap_err().contains("nested deeper"));
    }

    #[test]
    fn joins_relative_part_paths() {
        assert_eq!(
            join_part_path("ppt/slides", "../notesSlides/n1.xml"),
            "ppt/notesSlides/n1.xml"
        );
        assert_eq!(join_part_path("ppt", "slides/slide1.xml"), "ppt/slides/slide1.xml");
        assert_eq!(join_part_path("ppt", "/ppt/media/a.png"), "ppt/media/a.png");
    }
}
