//! Reads XLSX and XLSM files into the [`Workbook`] and [`Sheet`] model.
//!
//! An XLSX file is a zip of XML parts. This file reads the ones that hold
//! values: the workbook (sheet names, defined names), the shared strings, the
//! styles (to know which numbers are dates or percentages) and each sheet.
//! A sheet is read as a stream in one pass, so a 100,000-row sheet never
//! becomes an XML tree in memory. Pictures, charts and comments are read by
//! `read_xlsx_extras.rs`.

use std::collections::HashMap;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use super::model::{
    Cell, CellPosition, CellRange, CellValue, DefinedName, ExcelTable, SharedFormula, Sheet, SheetInfo, Workbook,
};
use super::read_xlsx_extras::{self, RichValuePictures};
use super::values::{NumberKind, builtin_format_kind, format_code_kind};
use crate::parsers::office_xml::{OfficeFile, XmlNode, event_text};

/// Days between Excel's two calendars: 1904-based files count from 1904-01-01.
const DAYS_FROM_1900_TO_1904: f64 = 1462.0;

/// An open XLSX file, ready to read its sheets one at a time.
pub struct XlsxReader<'bytes> {
    file: OfficeFile<'bytes>,
    workbook: Workbook,
    /// Where each sheet's XML is, in the same order as `workbook.sheets`.
    sheet_paths: Vec<String>,
    shared_strings: Vec<String>,
    /// For each cell style (`s="3"` on a cell), what it makes of a number.
    number_kinds: Vec<NumberKind>,
    is_1904: bool,
    /// Pictures placed in cells, read once for the whole workbook when the file has any.
    rich_value_pictures: Option<RichValuePictures>,
    /// The people who wrote threaded comments: id -> display name.
    people: HashMap<String, String>,
}

impl<'bytes> XlsxReader<'bytes> {
    /// Opens the file and reads the workbook-level parts.
    pub fn open(bytes: &'bytes [u8]) -> Result<XlsxReader<'bytes>, String> {
        let mut file = OfficeFile::open(bytes)?;
        let workbook_path = file
            .relationships("")?
            .into_values()
            .find(|(_, kind)| kind.ends_with("/officeDocument"))
            .map(|(path, _)| path)
            .unwrap_or_else(|| "xl/workbook.xml".to_string());
        let workbook_xml = file
            .xml(&workbook_path)?
            .ok_or_else(|| format!("{workbook_path} is missing: not an Excel workbook"))?;
        let workbook_links = file.relationships(&workbook_path)?;

        let is_1904 = workbook_xml
            .child("workbookPr")
            .and_then(|properties| properties.attribute("date1904"))
            .is_some_and(|value| value == "1" || value == "true");

        let mut workbook = Workbook::default();
        let mut sheet_paths = Vec::new();
        for sheet in workbook_xml
            .child("sheets")
            .map(|sheets| sheets.nodes().collect())
            .unwrap_or_else(Vec::new)
        {
            let (Some(name), Some(link)) = (sheet.attribute("name"), attribute_ending(sheet, "id")) else {
                continue;
            };
            let Some((path, kind)) = workbook_links.get(link) else {
                continue;
            };
            // Macro sheets and dialog sheets are skipped: foliq does not read macros.
            if !(kind.ends_with("/worksheet") || kind.ends_with("/chartsheet")) {
                continue;
            }
            let state = sheet.attribute("state").unwrap_or("visible");
            workbook.sheets.push(SheetInfo {
                name: name.to_string(),
                is_hidden: state == "hidden" || state == "veryHidden",
            });
            sheet_paths.push(path.clone());
        }
        workbook.defined_names = read_defined_names(&workbook_xml);

        let shared_strings = match find_link(&workbook_links, "/sharedStrings") {
            Some(path) => read_shared_strings(&file.text(&path)?.unwrap_or_default())?,
            None => Vec::new(),
        };
        let number_kinds = match find_link(&workbook_links, "/styles") {
            Some(path) => file
                .xml(&path)?
                .map(|styles| read_number_kinds(&styles))
                .unwrap_or_default(),
            None => Vec::new(),
        };
        let people = match find_link(&workbook_links, "/person") {
            Some(path) => read_xlsx_extras::read_people(&mut file, &path),
            None => HashMap::new(),
        };

        Ok(XlsxReader {
            file,
            workbook,
            sheet_paths,
            shared_strings,
            number_kinds,
            is_1904,
            rich_value_pictures: None,
            people,
        })
    }

    pub fn workbook(&self) -> &Workbook {
        &self.workbook
    }

    /// Reads the sheet at `index` (in `workbook().sheets`) completely.
    pub fn read_sheet(&mut self, index: usize) -> Result<Sheet, String> {
        let info = self.workbook.sheets[index].clone();
        let path = self.sheet_paths[index].clone();
        let xml = self.file.text(&path)?.ok_or_else(|| format!("{path} is missing"))?;
        let mut sheet = Sheet {
            name: info.name,
            is_hidden: info.is_hidden,
            ..Sheet::default()
        };
        let found = self.read_sheet_xml(&xml, &mut sheet)?;
        // Excel writes cells row by row; the rest of the parser relies on that order.
        sheet.cells.sort_by_key(|cell| cell.position);
        let links = self.file.relationships(&path)?;

        // Where each cell is in `sheet.cells`, to find cells by position quickly.
        let cell_index: HashMap<CellPosition, usize> =
            if found.hyperlinks.is_empty() && found.rich_value_cells.is_empty() {
                HashMap::new()
            } else {
                sheet
                    .cells
                    .iter()
                    .enumerate()
                    .map(|(index, cell)| (cell.position, index))
                    .collect()
            };

        // Hyperlinks point to their target through the sheet's relationships.
        for (range, link) in found.hyperlinks {
            let Some((target, _)) = links.get(&link) else { continue };
            for row in range.first.row..=range.last.row.min(range.first.row + 10_000) {
                for column in range.first.column..=range.last.column.min(range.first.column + 1_000) {
                    if let Some(&index) = cell_index.get(&CellPosition::new(row, column)) {
                        sheet.cells[index].link = Some(target.clone());
                    }
                }
            }
        }
        for link in found.table_links {
            if let Some((table_path, _)) = links.get(&link)
                && let Some(table) = self.file.xml(table_path)?.and_then(|xml| read_excel_table(&xml))
            {
                sheet.excel_tables.push(table);
            }
        }

        // The extras are best effort: a broken drawing or comment never loses the values.
        if !found.rich_value_cells.is_empty() {
            if self.rich_value_pictures.is_none() {
                self.rich_value_pictures = Some(read_xlsx_extras::read_rich_value_pictures(&mut self.file));
            }
            let pictures = self.rich_value_pictures.as_ref().expect("just read");
            for (position, value_metadata) in found.rich_value_cells {
                if let Some(picture) = pictures.picture_at(position, value_metadata) {
                    // The cell's saved value is only `#VALUE!` for older Excels: the picture replaces it.
                    if let Some(&index) = cell_index.get(&position) {
                        sheet.cells[index].value = CellValue::Empty;
                    }
                    sheet.pictures.push(picture);
                }
            }
        }
        read_xlsx_extras::read_sheet_extras(&mut self.file, &links, &self.people, &mut sheet);
        Ok(sheet)
    }

    /// Reads a sheet's XML in one pass: cells, merged ranges, and what links elsewhere.
    fn read_sheet_xml(&self, xml: &str, sheet: &mut Sheet) -> Result<SheetLinks, String> {
        let mut reader = Reader::from_str(xml);
        let mut found = SheetLinks::default();
        // Where the next cell is, for cells written without an `r="B2"` attribute.
        let mut next_position = CellPosition::new(0, 0);
        // The cell being read, between `<c>` and `</c>`.
        let mut cell: Option<CellBeingRead> = None;
        // Which text element we are inside, to know what the text belongs to.
        let mut inside = Inside::Nothing;

        loop {
            let event = reader.read_event().map_err(|error| error.to_string())?;
            match &event {
                Event::Start(tag) | Event::Empty(tag) => {
                    let is_empty = matches!(event, Event::Empty(_));
                    match tag.local_name().as_ref() {
                        "row" => {
                            if let Some(row) = raw_attribute(tag, "r").and_then(|row| row.parse::<u32>().ok()) {
                                next_position = CellPosition::new(row.saturating_sub(1), 0);
                            }
                        }
                        "c" => {
                            let mut started = CellBeingRead::default();
                            let mut position = None;
                            // One pass over the attributes: a big sheet has millions of cells.
                            for attribute in tag.attributes().with_checks(false).flatten() {
                                match attribute.key.as_ref() {
                                    "r" => position = CellPosition::parse(&attribute.value),
                                    "t" => started.kind = attribute.value.to_string(),
                                    "s" => started.style = attribute.value.parse().unwrap_or(0),
                                    "vm" => started.value_metadata = attribute.value.parse().ok(),
                                    _ => {}
                                }
                            }
                            started.position = position.unwrap_or(next_position);
                            next_position = CellPosition::new(started.position.row, started.position.column + 1);
                            if is_empty {
                                self.finish_cell(started, sheet, &mut found);
                            } else {
                                cell = Some(started);
                            }
                        }
                        "v" if !is_empty => inside = Inside::Value,
                        "f" => {
                            if let Some(cell) = cell.as_mut() {
                                cell.formula_kind = raw_attribute(tag, "t").unwrap_or_default();
                                cell.formula_range = raw_attribute(tag, "ref");
                            }
                            if !is_empty {
                                inside = Inside::Formula;
                            }
                        }
                        // Inline strings (`<is><t>`) and their rich-text runs; `rPh` is a
                        // phonetic reading guide (furigana), not part of the text.
                        "t" if !is_empty && inside != Inside::Phonetic => inside = Inside::InlineText,
                        "rPh" if !is_empty => inside = Inside::Phonetic,
                        "mergeCell" => {
                            if let Some(range) = raw_attribute(tag, "ref").and_then(|range| CellRange::parse(&range)) {
                                sheet.merged_ranges.push(range);
                            }
                        }
                        "hyperlink" => {
                            let range = raw_attribute(tag, "ref").and_then(|range| CellRange::parse(&range));
                            let link = tag_attribute_ending(tag, "id");
                            if let (Some(range), Some(link)) = (range, link) {
                                found.hyperlinks.push((range, link));
                            }
                        }
                        "tablePart" => {
                            if let Some(link) = tag_attribute_ending(tag, "id") {
                                found.table_links.push(link);
                            }
                        }
                        _ => {}
                    }
                }
                Event::End(tag) => match tag.local_name().as_ref() {
                    "c" => {
                        if let Some(finished) = cell.take() {
                            self.finish_cell(finished, sheet, &mut found);
                        }
                        if sheet.is_cut {
                            break;
                        }
                    }
                    "rPh" => inside = Inside::Nothing,
                    "v" | "f" | "t" if inside != Inside::Phonetic => inside = Inside::Nothing,
                    _ => {}
                },
                Event::Eof => break,
                other => {
                    if let (Some(cell), Some(text)) = (cell.as_mut(), event_text(other)?) {
                        match inside {
                            Inside::Value => cell.value.push_str(&text),
                            Inside::Formula => cell.formula.push_str(&text),
                            Inside::InlineText => cell.inline_text.push_str(&text),
                            Inside::Nothing | Inside::Phonetic => {}
                        }
                    }
                }
            }
        }
        Ok(found)
    }

    /// Turns a cell's raw parts into a [`Cell`], and keeps it if it holds anything.
    fn finish_cell(&self, raw: CellBeingRead, sheet: &mut Sheet, found: &mut SheetLinks) {
        let value = self.cell_value(&raw);
        if let Some(value_metadata) = raw.value_metadata {
            found.rich_value_cells.push((raw.position, value_metadata));
        }
        // A filled-down formula is written once, on the first cell of its range.
        let formula = if raw.formula.trim().is_empty() {
            None
        } else {
            Some(raw.formula.trim().to_string())
        };
        if raw.formula_kind == "shared"
            && let (Some(range), Some(formula)) = (raw.formula_range.as_deref().and_then(CellRange::parse), &formula)
            && range.first != range.last
        {
            sheet.shared_formulas.push(SharedFormula {
                range,
                formula: formula.clone(),
            });
        }
        if value.is_empty() && formula.is_none() && raw.value_metadata.is_none() {
            return;
        }
        sheet.add_cell(Cell {
            position: raw.position,
            value,
            formula,
            link: None,
        });
    }

    fn cell_value(&self, raw: &CellBeingRead) -> CellValue {
        let text = raw.value.as_str();
        match raw.kind.as_str() {
            "s" => match text
                .trim()
                .parse::<usize>()
                .ok()
                .and_then(|index| self.shared_strings.get(index))
            {
                Some(shared) => CellValue::Text(shared.clone()),
                None => CellValue::Empty,
            },
            "inlineStr" => CellValue::Text(raw.inline_text.clone()),
            "str" => CellValue::Text(text.to_string()),
            "b" => CellValue::Bool(text.trim() == "1" || text.trim() == "true"),
            "e" => CellValue::Error(text.trim().to_string()),
            // An ISO date written as text (`t="d"`), rare but allowed.
            "d" => CellValue::Text(text.trim().replace('T', " ")),
            _ => {
                let Ok(number) = text.trim().parse::<f64>() else {
                    // No saved value (a formula never calculated) or something unreadable.
                    return if raw.inline_text.is_empty() {
                        CellValue::Empty
                    } else {
                        CellValue::Text(raw.inline_text.clone())
                    };
                };
                match self.number_kinds.get(raw.style).copied().unwrap_or(NumberKind::Plain) {
                    NumberKind::Plain => CellValue::Number(number),
                    NumberKind::Percent => CellValue::Percent(number),
                    NumberKind::DateTime if self.is_1904 => CellValue::DateTime(number + DAYS_FROM_1900_TO_1904),
                    NumberKind::DateTime => CellValue::DateTime(number),
                }
            }
        }
    }
}

/// What a sheet's XML points to in other parts, resolved after the cells are read.
#[derive(Default)]
struct SheetLinks {
    /// Cell ranges and the relationship id of their link target.
    hyperlinks: Vec<(CellRange, String)>,
    /// Relationship ids of the sheet's Excel tables.
    table_links: Vec<String>,
    /// Cells holding a rich value (a picture placed in the cell), with their `vm` number.
    rich_value_cells: Vec<(CellPosition, u32)>,
}

/// The raw parts of a `<c>` element, collected while it is read.
#[derive(Default)]
struct CellBeingRead {
    position: CellPosition,
    /// The `t` attribute: `s` (shared string), `str`, `b`, `e`, `inlineStr`, or empty for a number.
    kind: String,
    /// The `s` attribute: an index into the styles.
    style: usize,
    /// The `vm` attribute: a rich value such as a picture placed in the cell.
    value_metadata: Option<u32>,
    value: String,
    inline_text: String,
    formula: String,
    formula_kind: String,
    formula_range: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Inside {
    Nothing,
    Value,
    Formula,
    InlineText,
    Phonetic,
}

/// An attribute of a tag, read as plain text. Fine for the short ASCII
/// attributes of cells (`r`, `s`, `t`, `vm`), which never hold `&` escapes.
fn raw_attribute(tag: &BytesStart, name: &str) -> Option<String> {
    for attribute in tag.attributes().with_checks(false).flatten() {
        if attribute.key.as_ref() == name {
            return Some(attribute.value.to_string());
        }
    }
    None
}

/// An attribute whose name ends in `:<local>`, like `r:id`. The prefix
/// (`r`) is chosen by whoever wrote the file, so only the end is compared.
fn tag_attribute_ending(tag: &BytesStart, local: &str) -> Option<String> {
    for attribute in tag.attributes().with_checks(false).flatten() {
        let key = attribute.key.as_ref().to_string();
        if key.ends_with(&format!(":{local}")) {
            return Some(attribute.value.to_string());
        }
    }
    None
}

/// Same as [`tag_attribute_ending`], on a node of an XML tree.
pub fn attribute_ending<'node>(node: &'node XmlNode, local: &str) -> Option<&'node str> {
    let suffix = format!(":{local}");
    node.attributes
        .iter()
        .find(|(key, _)| key.ends_with(&suffix))
        .map(|(_, value)| value.as_str())
}

/// The target of the first relationship whose type ends with `kind_ending`.
pub fn find_link(links: &HashMap<String, (String, String)>, kind_ending: &str) -> Option<String> {
    links
        .values()
        .find(|(_, kind)| kind.ends_with(kind_ending))
        .map(|(path, _)| path.clone())
}

/// The shared strings: every text cell points into this list by number.
fn read_shared_strings(xml: &str) -> Result<Vec<String>, String> {
    let mut reader = Reader::from_str(xml);
    let mut strings = Vec::new();
    let mut current = String::new();
    let mut in_text = false;
    let mut in_phonetic = false;
    loop {
        let event = reader.read_event().map_err(|error| format!("sharedStrings: {error}"))?;
        match &event {
            Event::Start(tag) => match tag.local_name().as_ref() {
                "si" => current.clear(),
                "t" => in_text = true,
                "rPh" => in_phonetic = true,
                _ => {}
            },
            Event::Empty(tag) if tag.local_name().as_ref() == "si" => strings.push(String::new()),
            Event::End(tag) => match tag.local_name().as_ref() {
                "si" => strings.push(std::mem::take(&mut current)),
                "t" => in_text = false,
                "rPh" => in_phonetic = false,
                _ => {}
            },
            Event::Eof => break,
            other => {
                if in_text
                    && !in_phonetic
                    && let Some(text) = event_text(other)?
                {
                    current.push_str(&text);
                }
            }
        }
    }
    Ok(strings)
}

/// For each cell style, whether it shows numbers as dates, percentages or plain numbers.
fn read_number_kinds(styles: &XmlNode) -> Vec<NumberKind> {
    let mut custom_formats: HashMap<u32, NumberKind> = HashMap::new();
    if let Some(formats) = styles.child("numFmts") {
        for format in formats.nodes() {
            if let (Some(id), Some(code)) = (format.attribute("numFmtId"), format.attribute("formatCode"))
                && let Ok(id) = id.parse()
            {
                custom_formats.insert(id, format_code_kind(code));
            }
        }
    }
    let Some(cell_styles) = styles.child("cellXfs") else {
        return Vec::new();
    };
    cell_styles
        .nodes()
        .map(|style| {
            let id: u32 = style.attribute("numFmtId").and_then(|id| id.parse().ok()).unwrap_or(0);
            custom_formats
                .get(&id)
                .copied()
                .unwrap_or_else(|| builtin_format_kind(id))
        })
        .collect()
}

/// Workbook-level names (`TaxRate`). Hidden names and Excel's own (`_xlnm.Print_Area`) are skipped.
fn read_defined_names(workbook: &XmlNode) -> Vec<DefinedName> {
    let Some(names) = workbook.child("definedNames") else {
        return Vec::new();
    };
    names
        .nodes()
        .filter(|name| name.attribute("hidden") != Some("1") && name.attribute("hidden") != Some("true"))
        .filter_map(|name| {
            let label = name.attribute("name")?;
            let formula = name.all_text().trim().to_string();
            if label.starts_with("_xlnm.")
                || label.starts_with("_xl")
                || formula.is_empty()
                || formula.contains("#REF!")
            {
                return None;
            }
            Some(DefinedName {
                name: label.to_string(),
                formula,
            })
        })
        .collect()
}

/// An Excel table part (`xl/tables/table1.xml`).
fn read_excel_table(table: &XmlNode) -> Option<ExcelTable> {
    let name = table.attribute("displayName").or(table.attribute("name"))?;
    let range = CellRange::parse(table.attribute("ref")?)?;
    let header_rows = table
        .attribute("headerRowCount")
        .and_then(|count| count.parse().ok())
        .unwrap_or(1);
    Some(ExcelTable {
        name: name.to_string(),
        range,
        header_rows,
    })
}
