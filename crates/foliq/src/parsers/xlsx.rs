//! Spreadsheets (XLSX, XLSM, XLS, ODS), read with `calamine`.
//!
//! Each sheet becomes a level-1 heading followed by a Markdown table, whose
//! first non-empty row is the header. Empty rows and columns are dropped.

use std::io::Cursor;

use calamine::{Data, Reader, open_workbook_auto_from_rs};
use foliq_core::{Error, Parsed, Parser, Result, SourceEntry};

use crate::markdown::MarkdownWriter;

pub struct XlsxParser;

impl Parser for XlsxParser {
    fn name(&self) -> &str {
        "xlsx"
    }

    fn extensions(&self) -> Vec<String> {
        super::extensions(&["xlsx", "xlsm", "xls", "ods"])
    }

    fn parse(&self, bytes: &[u8], entry: &SourceEntry) -> Result<Parsed> {
        let mut workbook =
            open_workbook_auto_from_rs(Cursor::new(bytes)).map_err(|error| Error::parse(&entry.path, error))?;
        let mut writer = MarkdownWriter::new();
        for sheet_name in workbook.sheet_names() {
            let sheet = workbook
                .worksheet_range(&sheet_name)
                .map_err(|error| Error::parse(&entry.path, error))?;
            let rows: Vec<Vec<String>> = sheet
                .rows()
                .map(|row| row.iter().map(cell_text).collect::<Vec<_>>())
                .filter(|row| row.iter().any(|cell| !cell.is_empty()))
                .collect();
            writer.heading(1, &sheet_name);
            writer.table(&drop_empty_columns(rows));
        }
        Ok(Parsed::markdown(writer.finish()))
    }
}

fn cell_text(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        // Show 30.0 as "30", like the spreadsheet does.
        Data::Float(number) if number.fract() == 0.0 && number.abs() < 1e15 => format!("{}", *number as i64),
        other => other.to_string(),
    }
}

fn drop_empty_columns(rows: Vec<Vec<String>>) -> Vec<Vec<String>> {
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    let used_columns: Vec<usize> = (0..width)
        .filter(|&column| {
            rows.iter()
                .any(|row| row.get(column).is_some_and(|cell| !cell.is_empty()))
        })
        .collect();
    rows.into_iter()
        .map(|row| {
            used_columns
                .iter()
                .map(|&column| row.get(column).cloned().unwrap_or_default())
                .collect()
        })
        .collect()
}
