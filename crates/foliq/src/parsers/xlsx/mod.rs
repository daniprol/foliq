//! Spreadsheets (XLSX, XLSM, XLSB, XLS, ODS).
//!
//! foliq reads what a person wrote or Excel computed: values, tables,
//! formulas, links, comments, pictures and charts. It ignores how the sheet
//! looks (styles, colours, number formats other than dates and percentages)
//! and how it runs (macros, recalculation).
//!
//! A reader fills the same small model for every format, then each sheet is
//! laid out and written, one sheet at a time:
//!
//! | File                  | Job                                                        |
//! |-----------------------|------------------------------------------------------------|
//! | `mod.rs`              | the parser: read the workbook, then each sheet in turn     |
//! | `model.rs`            | `Workbook`, `Sheet`, `Cell`... plain structs, no logic     |
//! | `read_xlsx.rs`        | XLSX and XLSM: values, formulas, merges, links, tables     |
//! | `read_xlsx_extras.rs` | XLSX pictures, charts, text boxes and comments             |
//! | `read_calamine.rs`    | XLS, XLSB and ODS with `calamine` (values and formulas)    |
//! | `values.rs`           | how values are written: plain numbers, ISO dates           |
//! | `layout.rs`           | finds the tables on a sheet                                |
//! | `formulas.rs`         | groups fill-down formulas into one line each               |
//! | `render.rs`           | writes the Markdown                                        |

mod formulas;
mod layout;
mod model;
mod read_calamine;
mod read_xlsx;
mod read_xlsx_extras;
mod render;
mod values;

use foliq_core::{Error, Parsed, Parser, Result, SourceEntry};

use self::model::{Sheet, Workbook};
use self::read_calamine::CalamineReader;
use self::read_xlsx::XlsxReader;
use super::office_xml::OfficeFile;
use crate::markdown::MarkdownWriter;

pub struct XlsxParser;

impl Parser for XlsxParser {
    fn name(&self) -> &str {
        "xlsx"
    }

    fn fingerprint(&self) -> String {
        // Version 2: tables found by layout, dates, formulas, pictures, comments, charts.
        "xlsx@2".to_string()
    }

    fn extensions(&self) -> Vec<String> {
        super::extensions(&["xlsx", "xlsm", "xlsb", "xls", "ods"])
    }

    fn parse(&self, bytes: &[u8], entry: &SourceEntry) -> Result<Parsed> {
        // XLSX and XLSM are zip files with an `xl/` folder: foliq reads those
        // itself. Everything else, or an XLSX it cannot read, goes to calamine.
        let own_error = match XlsxReader::open(bytes) {
            Ok(mut reader) => match write_workbook(&mut WorkbookReader::Xlsx(&mut reader)) {
                Ok(text) => return Ok(parsed(text, bytes)),
                Err(error) => Some(error),
            },
            Err(_) => None,
        };
        let mut reader = match CalamineReader::open(bytes) {
            Ok(reader) => reader,
            Err(error) => return Err(Error::parse(&entry.path, own_error.unwrap_or(error))),
        };
        match write_workbook(&mut WorkbookReader::Calamine(&mut reader)) {
            Ok(text) => Ok(parsed(text, bytes)),
            Err(error) => Err(Error::parse(&entry.path, own_error.unwrap_or(error))),
        }
    }
}

/// The Markdown, with the title from the file's properties when it has one.
fn parsed(text: String, bytes: &[u8]) -> Parsed {
    let mut parsed = Parsed::markdown(text);
    parsed.title = OfficeFile::open(bytes).ok().and_then(|mut file| file.title());
    parsed
}

/// One of the two readers. Both fill the same model, so the rest of the
/// parser never needs to know the file format.
enum WorkbookReader<'reader, 'bytes> {
    Xlsx(&'reader mut XlsxReader<'bytes>),
    Calamine(&'reader mut CalamineReader<'bytes>),
}

impl WorkbookReader<'_, '_> {
    fn workbook(&self) -> &Workbook {
        match self {
            WorkbookReader::Xlsx(reader) => reader.workbook(),
            WorkbookReader::Calamine(reader) => reader.workbook(),
        }
    }

    fn read_sheet(&mut self, index: usize) -> std::result::Result<Sheet, String> {
        match self {
            WorkbookReader::Xlsx(reader) => reader.read_sheet(index),
            WorkbookReader::Calamine(reader) => reader.read_sheet(index),
        }
    }
}

/// Reads and writes each sheet in turn, so only one is in memory at a time.
fn write_workbook(reader: &mut WorkbookReader) -> std::result::Result<String, String> {
    let mut writer = MarkdownWriter::new();
    for index in 0..reader.workbook().sheets.len() {
        let sheet = reader.read_sheet(index)?;
        render::write_sheet(&mut writer, &sheet);
    }
    render::write_names(&mut writer, &reader.workbook().defined_names);
    Ok(writer.finish())
}

#[cfg(test)]
mod tests;
