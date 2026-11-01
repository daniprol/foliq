//! Reads the other spreadsheet formats (XLS, XLSB, ODS) with `calamine`.
//!
//! These get values and formulas. Pictures, charts and comments are only
//! read from XLSX. This reader is also the fallback for an XLSX file that
//! foliq's own reader cannot open.

use std::io::Cursor;

use calamine::{Data, Reader, SheetType, SheetVisible, Sheets, open_workbook_auto_from_rs};

use super::model::{Cell, CellPosition, CellValue, DefinedName, Sheet, SheetInfo, Workbook};

/// An open workbook in any format calamine knows.
pub struct CalamineReader<'bytes> {
    sheets: Sheets<Cursor<&'bytes [u8]>>,
    workbook: Workbook,
}

impl<'bytes> CalamineReader<'bytes> {
    pub fn open(bytes: &'bytes [u8]) -> Result<CalamineReader<'bytes>, String> {
        let sheets = open_workbook_auto_from_rs(Cursor::new(bytes)).map_err(|error| error.to_string())?;
        let mut workbook = Workbook::default();
        for sheet in sheets.sheets_metadata() {
            // Macro and dialog sheets are skipped: foliq does not read macros.
            if matches!(sheet.typ, SheetType::WorkSheet | SheetType::ChartSheet) {
                workbook.sheets.push(SheetInfo {
                    name: sheet.name.clone(),
                    is_hidden: sheet.visible != SheetVisible::Visible,
                });
            }
        }
        workbook.defined_names = sheets
            .defined_names()
            .iter()
            .filter(|(name, formula)| !name.starts_with("_xlnm.") && !formula.contains("#REF!"))
            .map(|(name, formula)| DefinedName {
                name: name.clone(),
                formula: formula.clone(),
            })
            .collect();
        Ok(CalamineReader { sheets, workbook })
    }

    pub fn workbook(&self) -> &Workbook {
        &self.workbook
    }

    pub fn read_sheet(&mut self, index: usize) -> Result<Sheet, String> {
        let info = self.workbook.sheets[index].clone();
        let mut sheet = Sheet {
            name: info.name.clone(),
            is_hidden: info.is_hidden,
            ..Sheet::default()
        };
        let values = self
            .sheets
            .worksheet_range(&info.name)
            .map_err(|error| error.to_string())?;
        // A range starts at its first used cell, not at A1: `cells()` counts from there.
        let (first_row, first_column) = values.start().unwrap_or((0, 0));
        for (row, column, data) in values.cells() {
            let value = cell_value(data);
            if !value.is_empty() {
                sheet.add_cell(Cell {
                    position: CellPosition::new(first_row + row as u32, first_column + column as u32),
                    value,
                    formula: None,
                    link: None,
                });
            }
        }
        // Formulas are a second grid; a sheet whose formulas can't be read keeps its values.
        if let Ok(formulas) = self.sheets.worksheet_formula(&info.name) {
            let (first_row, first_column) = formulas.start().unwrap_or((0, 0));
            let mut index_of = std::collections::HashMap::new();
            for (index, cell) in sheet.cells.iter().enumerate() {
                index_of.insert(cell.position, index);
            }
            for (row, column, formula) in formulas.cells() {
                if formula.trim().is_empty() {
                    continue;
                }
                let position = CellPosition::new(first_row + row as u32, first_column + column as u32);
                match index_of.get(&position) {
                    Some(&index) => sheet.cells[index].formula = Some(formula.clone()),
                    None => sheet.cells.push(Cell {
                        position,
                        value: CellValue::Empty,
                        formula: Some(formula.clone()),
                        link: None,
                    }),
                }
            }
            sheet.cells.sort_by_key(|cell| cell.position);
        }
        Ok(sheet)
    }
}

fn cell_value(data: &Data) -> CellValue {
    match data {
        Data::Empty => CellValue::Empty,
        Data::String(text) => CellValue::Text(text.clone()),
        Data::Int(number) => CellValue::Number(*number as f64),
        Data::Float(number) => CellValue::Number(*number),
        Data::Bool(value) => CellValue::Bool(*value),
        Data::DateTime(date) if date.is_datetime() => CellValue::DateTime(date.as_f64()),
        Data::DateTime(duration) => CellValue::Number(duration.as_f64()),
        Data::DateTimeIso(text) | Data::DurationIso(text) => CellValue::Text(text.replace('T', " ")),
        Data::Error(error) => CellValue::Error(error.to_string()),
    }
}
