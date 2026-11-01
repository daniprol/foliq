//! Reads what an XLSX sheet holds besides cell values: pictures, charts,
//! text boxes and comments.
//!
//! These live in their own small parts, linked from the sheet:
//!
//! | Part                              | What foliq takes from it                      |
//! |-----------------------------------|-----------------------------------------------|
//! | `xl/drawings/drawing1.xml`        | floating pictures (with alt text), charts, text boxes |
//! | `xl/charts/chart1.xml`            | a chart's title, kind and data ranges          |
//! | `xl/comments1.xml`                | notes (Excel's "comments" before 2019)         |
//! | `xl/threadedComments/*.xml`       | comment threads, with replies                  |
//! | `xl/metadata.xml`, `xl/richData/` | pictures placed in a cell ("Place in Cell")    |
//!
//! Everything here is best effort: a part that is missing or broken is
//! skipped, and the sheet's values are still read.

use std::collections::HashMap;

use super::model::{CellPosition, Chart, ChartSeries, Note, NoteKind, Picture, Sheet};
use super::read_xlsx::attribute_ending;
use crate::parsers::office_xml::{OfficeFile, XmlNode};

/// The relationships of a part: id -> (target path, type).
type Links = HashMap<String, (String, String)>;

/// Adds the pictures, charts, text boxes and comments linked from a sheet.
pub fn read_sheet_extras(
    file: &mut OfficeFile,
    sheet_links: &Links,
    people: &HashMap<String, String>,
    sheet: &mut Sheet,
) {
    for (path, kind) in sheet_links.values() {
        if kind.ends_with("/drawing") {
            read_drawing(file, path, sheet);
        }
    }
    // Threaded comments come first: Excel also writes each of them as an old-style
    // note ("[Threaded comment] ..."), which is skipped for those cells.
    for (path, kind) in sheet_links.values() {
        if kind.ends_with("/threadedComment") {
            read_threaded_comments(file, path, people, sheet);
        }
    }
    for (path, kind) in sheet_links.values() {
        if kind.ends_with("/comments") {
            read_comments(file, path, sheet);
        }
    }
    sheet.notes.sort_by_key(|note| note.cell);
}

/// Pictures, charts and text boxes, each anchored to the cell under its top-left corner.
fn read_drawing(file: &mut OfficeFile, path: &str, sheet: &mut Sheet) {
    let Ok(Some(drawing)) = file.xml(path) else { return };
    let links = file.relationships(path).unwrap_or_default();
    for anchor in drawing.nodes() {
        let cell = anchor_cell(anchor);
        for picture in anchor.descendants("pic") {
            let description = picture
                .find(&["nvPicPr", "cNvPr"])
                .and_then(|properties| properties.attribute("descr"))
                .map(|text| text.trim().to_string())
                .filter(|text| !text.is_empty());
            let image_link = picture
                .descendants("blip")
                .first()
                .and_then(|blip| attribute_ending(blip, "embed"));
            if let Some((part_path, _)) = image_link.and_then(|link| links.get(link)) {
                sheet.pictures.push(Picture {
                    anchor: cell,
                    part_path: part_path.clone(),
                    description,
                });
            }
        }
        for chart_reference in anchor.descendants("chart") {
            let Some((chart_path, _)) = attribute_ending(chart_reference, "id").and_then(|link| links.get(link)) else {
                continue;
            };
            if let Ok(Some(chart)) = file.xml(chart_path)
                && let Some(chart) = read_chart(&chart, cell)
            {
                sheet.charts.push(chart);
            }
        }
        for shape in anchor.descendants("sp") {
            let text = shape.child("txBody").map(paragraphs_text).unwrap_or_default();
            if !text.is_empty() {
                sheet.notes.push(Note {
                    kind: NoteKind::TextBox,
                    cell,
                    author: None,
                    date: None,
                    text,
                    replies: Vec::new(),
                });
            }
        }
    }
}

/// The cell under an anchor's top-left corner (`<xdr:from><xdr:col>3</xdr:col><xdr:row>5</xdr:row>`).
fn anchor_cell(anchor: &XmlNode) -> CellPosition {
    let number = |name: &str| -> u32 {
        anchor
            .find(&["from", name])
            .and_then(|node| node.all_text().trim().parse().ok())
            .unwrap_or(0)
    };
    CellPosition::new(number("row"), number("col"))
}

/// The text of a text body, one line per paragraph, joined with spaces.
fn paragraphs_text(body: &XmlNode) -> String {
    body.descendants("p")
        .iter()
        .map(|paragraph| paragraph.office_text().trim().to_string())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// A chart part: its title, kind and the cell ranges of each series.
fn read_chart(chart_space: &XmlNode, anchor: CellPosition) -> Option<Chart> {
    let chart = chart_space.child("chart")?;
    let title = chart.child("title").map(chart_text).filter(|title| !title.is_empty());
    let plot_area = chart.child("plotArea")?;
    let mut kind = None;
    let mut series = Vec::new();
    for plot in plot_area.nodes().filter(|node| node.name.ends_with("Chart")) {
        if kind.is_none() {
            kind = Some(chart_kind(plot));
        }
        for one_series in plot.nodes().filter(|node| node.name == "ser") {
            let formula = |path: &[&str]| -> Option<String> {
                let node = one_series.find(path)?;
                let reference = node.descendants("f").first()?.all_text();
                let reference = reference.trim();
                (!reference.is_empty()).then(|| reference.replace('$', ""))
            };
            series.push(ChartSeries {
                name: one_series.child("tx").map(chart_text).filter(|name| !name.is_empty()),
                values: formula(&["val"]).or_else(|| formula(&["yVal"])),
                categories: formula(&["cat"]).or_else(|| formula(&["xVal"])),
            });
        }
    }
    Some(Chart {
        anchor,
        title,
        kind: kind.unwrap_or_else(|| "chart".to_string()),
        series,
    })
}

/// `lineChart` is `line`; a `barChart` with `barDir="col"` is `column`.
fn chart_kind(plot: &XmlNode) -> String {
    let name = plot.name.trim_end_matches("Chart").trim_end_matches("3D");
    if name == "bar" && plot.child("barDir").and_then(|direction| direction.attribute("val")) == Some("col") {
        return "column".to_string();
    }
    name.to_string()
}

/// The words of a title or series name: rich text (`<a:t>`), else the cached value (`<c:v>`).
fn chart_text(node: &XmlNode) -> String {
    let rich = node.office_text();
    if !rich.trim().is_empty() {
        return rich.split_whitespace().collect::<Vec<_>>().join(" ");
    }
    node.descendants("v")
        .iter()
        .map(|value| value.all_text().trim().to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Old-style notes: `<comment ref="B4" authorId="0"><text>...</text></comment>`.
fn read_comments(file: &mut OfficeFile, path: &str, sheet: &mut Sheet) {
    let Ok(Some(comments)) = file.xml(path) else { return };
    let authors: Vec<String> = comments
        .child("authors")
        .map(|authors| {
            authors
                .nodes()
                .map(|author| author.all_text().trim().to_string())
                .collect()
        })
        .unwrap_or_default();
    let Some(list) = comments.child("commentList") else {
        return;
    };
    for comment in list.nodes() {
        let Some(cell) = comment.attribute("ref").and_then(CellPosition::parse) else {
            continue;
        };
        let has_thread = sheet
            .notes
            .iter()
            .any(|note| note.kind == NoteKind::Comment && note.cell == cell);
        if has_thread {
            continue;
        }
        let author = comment
            .attribute("authorId")
            .and_then(|id| id.parse::<usize>().ok())
            .and_then(|id| authors.get(id))
            .filter(|author| !author.is_empty() && !author.starts_with("tc="))
            .cloned();
        let mut text = comment.child("text").map(|text| text.office_text()).unwrap_or_default();
        // Excel starts a note with its author's name in bold: "Ana:\nCheck with supplier".
        if let Some(author) = &author
            && let Some(rest) = text.trim_start().strip_prefix(author.as_str())
            && let Some(rest) = rest.strip_prefix(':')
        {
            text = rest.to_string();
        }
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if !text.is_empty() {
            sheet.notes.push(Note {
                kind: NoteKind::Comment,
                cell,
                author,
                date: None,
                text,
                replies: Vec::new(),
            });
        }
    }
}

/// Comment threads (Excel 365): a first comment and its replies.
fn read_threaded_comments(file: &mut OfficeFile, path: &str, people: &HashMap<String, String>, sheet: &mut Sheet) {
    let Ok(Some(comments)) = file.xml(path) else { return };
    // Comment id -> index of its thread in `threads`.
    let mut thread_of: HashMap<String, usize> = HashMap::new();
    let mut threads: Vec<Note> = Vec::new();
    for comment in comments.nodes() {
        let Some(cell) = comment.attribute("ref").and_then(CellPosition::parse) else {
            continue;
        };
        let text = comment.child("text").map(|text| text.all_text()).unwrap_or_default();
        let note = Note {
            kind: NoteKind::Comment,
            cell,
            author: comment.attribute("personId").and_then(|id| people.get(id)).cloned(),
            // `dT` is like `2026-06-18T17:12:37.41`: keep the date.
            date: comment.attribute("dT").map(|time| time.chars().take(10).collect()),
            text: text.split_whitespace().collect::<Vec<_>>().join(" "),
            replies: Vec::new(),
        };
        let id = comment.attribute("id").unwrap_or("").to_string();
        match comment
            .attribute("parentId")
            .and_then(|parent| thread_of.get(parent))
            .copied()
        {
            Some(thread) => {
                threads[thread].replies.push(note);
                thread_of.insert(id, thread);
            }
            None => {
                thread_of.insert(id, threads.len());
                threads.push(note);
            }
        }
    }
    sheet.notes.extend(threads);
}

/// The people who wrote threaded comments: id -> display name (`xl/persons/person.xml`).
pub fn read_people(file: &mut OfficeFile, path: &str) -> HashMap<String, String> {
    let mut people = HashMap::new();
    if let Ok(Some(list)) = file.xml(path) {
        for person in list.nodes() {
            if let (Some(id), Some(name)) = (person.attribute("id"), person.attribute("displayName")) {
                people.insert(id.to_string(), name.to_string());
            }
        }
    }
    people
}

/// Pictures placed in cells, found through Excel's rich values.
///
/// A cell with a picture has `vm="1"`. That number leads, through
/// `metadata.xml`, to a rich value in `richData/rdrichvalue.xml`, which holds
/// the picture's alt text and a number that `richData/richValueRel.xml`
/// turns into the image part (`xl/media/image1.png`).
#[derive(Default)]
pub struct RichValuePictures {
    /// For each `vm` number (from 1), the rich value it points to.
    rich_value_of_metadata: Vec<Option<usize>>,
    /// For each rich value: its image part and alt text, if it is a picture.
    pictures: Vec<Option<(String, Option<String>)>>,
}

impl RichValuePictures {
    pub fn picture_at(&self, cell: CellPosition, value_metadata: u32) -> Option<Picture> {
        let index = (value_metadata as usize).checked_sub(1)?;
        let rich_value = (*self.rich_value_of_metadata.get(index)?)?;
        let (part_path, description) = self.pictures.get(rich_value)?.clone()?;
        Some(Picture {
            anchor: cell,
            part_path,
            description,
        })
    }
}

pub fn read_rich_value_pictures(file: &mut OfficeFile) -> RichValuePictures {
    let mut result = RichValuePictures::default();
    let Ok(Some(metadata)) = file.xml("xl/metadata.xml") else {
        return result;
    };
    // `<rc t="1" v="0"/>`: `t` numbers a metadata type from 1, `v` a block of that type from 0.
    let rich_type = metadata
        .child("metadataTypes")
        .and_then(|types| {
            types
                .nodes()
                .position(|kind| kind.attribute("name") == Some("XLRICHVALUE"))
        })
        .map(|index| (index + 1).to_string());
    let future_blocks: Vec<Option<usize>> = metadata
        .nodes()
        .find(|node| node.name == "futureMetadata" && node.attribute("name") == Some("XLRICHVALUE"))
        .map(|future| {
            future
                .nodes()
                .map(|block| {
                    block
                        .descendants("rvb")
                        .first()
                        .and_then(|rich| rich.attribute("i"))
                        .and_then(|index| index.parse().ok())
                })
                .collect()
        })
        .unwrap_or_default();
    if let Some(values) = metadata.child("valueMetadata") {
        for block in values.nodes() {
            let rich_value = block
                .nodes()
                .find(|record| record.attribute("t").map(str::to_string) == rich_type)
                .and_then(|record| record.attribute("v"))
                .and_then(|index| index.parse::<usize>().ok())
                .and_then(|index| future_blocks.get(index).copied().flatten());
            result.rich_value_of_metadata.push(rich_value);
        }
    }

    // The keys of each rich value structure, to find the image and the alt text by name.
    let structures: Vec<Vec<String>> = match file.xml("xl/richData/rdrichvaluestructure.xml") {
        Ok(Some(structures)) => structures
            .nodes()
            .map(|structure| {
                structure
                    .nodes()
                    .map(|key| key.attribute("n").unwrap_or("").to_string())
                    .collect()
            })
            .collect(),
        _ => Vec::new(),
    };
    let image_parts: Vec<Option<String>> = {
        let links = file.relationships("xl/richData/richValueRel.xml").unwrap_or_default();
        match file.xml("xl/richData/richValueRel.xml") {
            Ok(Some(relations)) => relations
                .nodes()
                .map(|relation| {
                    attribute_ending(relation, "id")
                        .and_then(|id| links.get(id))
                        .map(|(path, _)| path.clone())
                })
                .collect(),
            _ => Vec::new(),
        }
    };
    let Ok(Some(rich_values)) = file.xml("xl/richData/rdrichvalue.xml") else {
        return result;
    };
    for rich_value in rich_values.nodes() {
        let values: Vec<String> = rich_value.nodes().map(|value| value.all_text()).collect();
        let keys = rich_value
            .attribute("s")
            .and_then(|index| index.parse::<usize>().ok())
            .and_then(|index| structures.get(index));
        let value_of = |key: &str| -> Option<&String> {
            let position = keys?.iter().position(|name| name == key)?;
            values.get(position)
        };
        let image = value_of("_rvRel:LocalImageIdentifier")
            .and_then(|index| index.trim().parse::<usize>().ok())
            .and_then(|index| image_parts.get(index).cloned().flatten());
        let description = value_of("Text")
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty());
        result.pictures.push(image.map(|path| (path, description)));
    }
    result
}
