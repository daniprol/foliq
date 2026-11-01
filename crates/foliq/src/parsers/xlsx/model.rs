//! The workbook model: what foliq keeps from a spreadsheet, whatever its format.
//!
//! The readers (`read_xlsx.rs`, `read_calamine.rs`) fill these plain structs,
//! and `layout.rs` and `render.rs` turn them into Markdown. Only one [`Sheet`]
//! is in memory at a time: it is read, written out, then dropped.
//!
//! Rows and columns count from 0, like Python lists. Excel shows them from 1
//! (`A1` is row 0, column 0).

/// The parts of a workbook that belong to the whole file.
#[derive(Debug, Default)]
pub struct Workbook {
    /// The sheets, in the order Excel shows their tabs.
    pub sheets: Vec<SheetInfo>,
    /// Named cells and ranges, like `TaxRate = Settings!$B$2`.
    pub defined_names: Vec<DefinedName>,
}

/// A sheet's name and whether it is hidden, known before the sheet is read.
#[derive(Debug, Clone)]
pub struct SheetInfo {
    pub name: String,
    pub is_hidden: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DefinedName {
    pub name: String,
    /// What the name stands for: a range (`Settings!$B$2`), a constant or a formula.
    pub formula: String,
}

/// One sheet, read completely before it is written out.
#[derive(Debug, Default)]
pub struct Sheet {
    pub name: String,
    pub is_hidden: bool,
    /// The cells that hold a value or a formula, row by row. Empty cells are not stored.
    pub cells: Vec<Cell>,
    /// Merged cells: the value is in the top-left cell of each range.
    pub merged_ranges: Vec<CellRange>,
    /// Excel tables (Insert > Table), with their names.
    pub excel_tables: Vec<ExcelTable>,
    /// Groups of cells that share one formula, filled down or across (`D2:D500 = B2*C2`).
    pub shared_formulas: Vec<SharedFormula>,
    pub pictures: Vec<Picture>,
    pub charts: Vec<Chart>,
    /// Comments, notes and text boxes.
    pub notes: Vec<Note>,
    /// Whether the sheet held more text than foliq reads ([`MAX_SHEET_TEXT_BYTES`]),
    /// so its last cells were skipped.
    pub is_cut: bool,
    /// The bytes of cell text read so far, to know when to stop.
    pub text_bytes: usize,
}

/// The most text Excel keeps in one cell. Longer text only comes from broken
/// or hostile files, and is cut.
pub const MAX_CELL_CHARACTERS: usize = 32_767;

/// The most cell text read from one sheet (10 MB). A sheet that repeats one
/// huge text in every cell could otherwise fill the memory.
pub const MAX_SHEET_TEXT_BYTES: usize = 10_000_000;

impl Sheet {
    /// Adds a cell, unless the sheet already holds all the text foliq reads.
    pub fn add_cell(&mut self, mut cell: Cell) {
        if self.is_cut {
            return;
        }
        if let CellValue::Text(text) = &mut cell.value
            && text.len() > MAX_CELL_CHARACTERS
        {
            *text = text.chars().take(MAX_CELL_CHARACTERS).collect();
        }
        if let CellValue::Text(text) = &cell.value {
            self.text_bytes += text.len();
        }
        if self.text_bytes > MAX_SHEET_TEXT_BYTES {
            self.is_cut = true;
            return;
        }
        self.cells.push(cell);
    }
}

/// A cell that holds something.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub position: CellPosition,
    pub value: CellValue,
    /// The formula without its `=`, if the cell is computed. `Option` is like
    /// Python's `Optional[str]`: `Some("B2*C2")` or `None`.
    pub formula: Option<String>,
    /// Where a hyperlink on the cell points.
    pub link: Option<String>,
}

/// A cell's value, as Excel last computed and saved it.
///
/// An `enum` is a value that is exactly one of these kinds, each with its own
/// data, like a tagged union (or a small class hierarchy in Python).
#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    Empty,
    Text(String),
    Number(f64),
    /// A number shown as a percentage (`0.25` shown as `25%`).
    Percent(f64),
    Bool(bool),
    /// A date or time, as Excel stores it: days since 1899-12-30, with the time as a fraction.
    DateTime(f64),
    /// A formula error, like `#N/A` or `#DIV/0!`.
    Error(String),
}

impl CellValue {
    pub fn is_empty(&self) -> bool {
        match self {
            CellValue::Empty => true,
            CellValue::Text(text) => text.trim().is_empty(),
            _ => false,
        }
    }
}

/// A cell's place on its sheet, counted from 0.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellPosition {
    pub row: u32,
    pub column: u32,
}

impl CellPosition {
    pub fn new(row: u32, column: u32) -> CellPosition {
        CellPosition { row, column }
    }

    /// Reads an Excel reference like `B12` or `$B$12`.
    pub fn parse(reference: &str) -> Option<CellPosition> {
        // Byte by byte, without building strings: big sheets parse millions of these.
        let mut column: u32 = 0;
        let mut letters = 0;
        let mut row: u32 = 0;
        let mut digits = 0;
        for byte in reference.trim().bytes() {
            match byte {
                b'$' => {}
                b'A'..=b'Z' | b'a'..=b'z' if digits == 0 => {
                    column = column * 26 + u32::from(byte.to_ascii_uppercase() - b'A' + 1);
                    letters += 1;
                }
                b'0'..=b'9' if letters > 0 => {
                    row = row.checked_mul(10)?.checked_add(u32::from(byte - b'0'))?;
                    digits += 1;
                }
                _ => return None,
            }
        }
        if letters == 0 || letters > 3 || digits == 0 || row == 0 {
            return None;
        }
        Some(CellPosition::new(row - 1, column - 1))
    }

    /// The Excel name of the cell: row 0, column 1 is `B1`.
    pub fn name(&self) -> String {
        format!("{}{}", column_letters(self.column), self.row + 1)
    }
}

/// The letters of a column: 0 is `A`, 26 is `AA`.
pub fn column_letters(column: u32) -> String {
    let mut letters = Vec::new();
    let mut number = column + 1;
    while number > 0 {
        let remainder = (number - 1) % 26;
        letters.push((b'A' + remainder as u8) as char);
        number = (number - 1) / 26;
    }
    letters.iter().rev().collect()
}

/// A rectangle of cells, both corners included, like `A1:D9`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellRange {
    pub first: CellPosition,
    pub last: CellPosition,
}

impl CellRange {
    pub fn new(first: CellPosition, last: CellPosition) -> CellRange {
        CellRange { first, last }
    }

    /// Reads `A1:D9`, or a single cell `B2`.
    pub fn parse(reference: &str) -> Option<CellRange> {
        // A range may name its sheet (`Sales!A1:B2`): keep only the cells.
        let cells = reference.rsplit('!').next()?;
        match cells.split_once(':') {
            Some((first, last)) => {
                let first = CellPosition::parse(first)?;
                let last = CellPosition::parse(last)?;
                Some(CellRange::new(
                    CellPosition::new(first.row.min(last.row), first.column.min(last.column)),
                    CellPosition::new(first.row.max(last.row), first.column.max(last.column)),
                ))
            }
            None => {
                let cell = CellPosition::parse(cells)?;
                Some(CellRange::new(cell, cell))
            }
        }
    }

    pub fn contains(&self, position: CellPosition) -> bool {
        self.first.row <= position.row
            && position.row <= self.last.row
            && self.first.column <= position.column
            && position.column <= self.last.column
    }

    pub fn row_count(&self) -> u32 {
        self.last.row - self.first.row + 1
    }

    pub fn column_count(&self) -> u32 {
        self.last.column - self.first.column + 1
    }

    /// `A1:D9`, or `B2` for a single cell.
    pub fn name(&self) -> String {
        if self.first == self.last {
            self.first.name()
        } else {
            format!("{}:{}", self.first.name(), self.last.name())
        }
    }
}

/// A table made with Insert > Table in Excel.
#[derive(Debug, Clone, PartialEq)]
pub struct ExcelTable {
    pub name: String,
    pub range: CellRange,
    /// How many rows at the top are headers (usually 1, sometimes 0).
    pub header_rows: u32,
}

/// One formula filled over a range: Excel writes it once, on the first cell.
#[derive(Debug, Clone, PartialEq)]
pub struct SharedFormula {
    pub range: CellRange,
    /// The formula as written in the first cell of the range.
    pub formula: String,
}

/// A picture, floating over the sheet or placed in a cell.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    /// The cell under the picture's top-left corner (or the cell holding it).
    pub anchor: CellPosition,
    /// Where the image is inside the file: `xl/media/image1.png`.
    pub part_path: String,
    /// The picture's alt text, if someone wrote one.
    pub description: Option<String>,
}

/// A chart drawn on the sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct Chart {
    pub anchor: CellPosition,
    pub title: Option<String>,
    /// `line`, `bar`, `column`, `pie`...
    pub kind: String,
    pub series: Vec<ChartSeries>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChartSeries {
    pub name: Option<String>,
    /// The cells that hold the values: `Sales!$F$2:$F$13`.
    pub values: Option<String>,
    /// The cells that hold the labels (the x axis): `Sales!$A$2:$A$13`.
    pub categories: Option<String>,
}

/// A comment or note on a cell, or the text of a text box.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub kind: NoteKind,
    /// The cell the note belongs to (for a text box, the cell under its corner).
    pub cell: CellPosition,
    pub author: Option<String>,
    /// When it was written, as `YYYY-MM-DD`, if the file says.
    pub date: Option<String>,
    pub text: String,
    /// Answers in a comment thread.
    pub replies: Vec<Note>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    Comment,
    TextBox,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_names_go_both_ways() {
        for name in ["A1", "Z9", "AA10", "XFD1048576"] {
            assert_eq!(CellPosition::parse(name).unwrap().name(), name);
        }
        assert_eq!(CellPosition::parse("$B$2"), Some(CellPosition::new(1, 1)));
        assert_eq!(CellPosition::parse("B0"), None);
        assert_eq!(CellRange::parse("Sales!$D$9:A1").unwrap().name(), "A1:D9");
    }
}
