//! Finds the tables on a sheet.
//!
//! A sheet often holds more than one table, or a title above a table, with
//! empty cells between them. The rule is simple: **cells that touch form one
//! table**. Excel tables (Insert > Table) are taken as they are first. Then
//! every group of filled cells connected through their sides (not corners)
//! becomes a table, and groups whose rectangles overlap are joined. The first
//! row of each table is its header.
//!
//! The cells are kept sorted row by row, so neighbours are found with a binary
//! search instead of a hash table: a 680,000-cell sheet takes a few dozen ms.

use std::collections::HashSet;

use super::model::{CellPosition, CellRange, Sheet};

/// A rectangle of cells that reads as one table.
#[derive(Debug, Clone, PartialEq)]
pub struct TableRegion {
    pub range: CellRange,
    /// The Excel table's name, for tables made with Insert > Table.
    pub name: Option<String>,
    /// How many rows at the top are headers.
    pub header_rows: u32,
}

/// The tables of a sheet, top to bottom, then left to right.
pub fn find_tables(sheet: &Sheet) -> Vec<TableRegion> {
    let mut regions: Vec<TableRegion> = sheet
        .excel_tables
        .iter()
        .map(|table| TableRegion {
            range: table.range,
            name: Some(table.name.clone()),
            header_rows: table.header_rows,
        })
        .collect();

    // The filled cells outside Excel tables, sorted row by row (like reading order).
    let mut filled: Vec<CellPosition> = filled_cells(sheet)
        .into_iter()
        .filter(|cell| !regions.iter().any(|region| region.range.contains(*cell)))
        .collect();
    filled.sort();
    filled.dedup();

    // Join each cell with its left and upper neighbours ("union-find": every
    // cell points to a cell of its group, and `group_of` follows the pointers
    // to the group's first cell).
    let mut parent: Vec<usize> = (0..filled.len()).collect();
    for index in 0..filled.len() {
        let cell = filled[index];
        if index > 0 && filled[index - 1].row == cell.row && filled[index - 1].column + 1 == cell.column {
            join(&mut parent, index - 1, index);
        }
        if cell.row > 0
            && let Ok(above) = filled.binary_search(&CellPosition::new(cell.row - 1, cell.column))
        {
            join(&mut parent, above, index);
        }
    }

    // The rectangle of each group.
    let mut bounds: Vec<Option<CellRange>> = vec![None; filled.len()];
    for (index, &cell) in filled.iter().enumerate() {
        let group = group_of(&mut parent, index);
        bounds[group] = Some(match bounds[group] {
            Some(range) => grow(range, cell),
            None => CellRange::new(cell, cell),
        });
    }
    let groups: Vec<CellRange> = bounds.into_iter().flatten().collect();

    for range in join_overlapping(groups) {
        regions.push(TableRegion {
            range,
            name: None,
            header_rows: 1,
        });
    }
    regions.sort_by_key(|region| region.range.first);
    regions
}

/// The first cell of the group `index` belongs to. Shortens the path on the way.
fn group_of(parent: &mut [usize], index: usize) -> usize {
    let mut root = index;
    while parent[root] != root {
        root = parent[root];
    }
    let mut current = index;
    while parent[current] != root {
        let next = parent[current];
        parent[current] = root;
        current = next;
    }
    root
}

fn join(parent: &mut [usize], first: usize, second: usize) {
    let first_group = group_of(parent, first);
    let second_group = group_of(parent, second);
    // The group keeps its earliest cell as its name, so groups stay in reading order.
    let (keep, other) = if first_group < second_group {
        (first_group, second_group)
    } else {
        (second_group, first_group)
    };
    parent[other] = keep;
}

/// The cells that count as filled: those with a value, a formula or a picture,
/// and every cell of a merged range whose top-left cell is filled.
fn filled_cells(sheet: &Sheet) -> Vec<CellPosition> {
    let mut filled: Vec<CellPosition> = sheet
        .cells
        .iter()
        .filter(|cell| !cell.value.is_empty() || cell.formula.is_some())
        .map(|cell| cell.position)
        .collect();
    filled.extend(sheet.pictures.iter().map(|picture| picture.anchor));
    let starts: HashSet<CellPosition> = sheet.merged_ranges.iter().map(|merged| merged.first).collect();
    let filled_starts: HashSet<CellPosition> = filled.iter().filter(|cell| starts.contains(cell)).copied().collect();
    for merged in &sheet.merged_ranges {
        // A merge can cover a whole row; only its first 1,000 rows and columns are counted.
        if filled_starts.contains(&merged.first) {
            for row in merged.first.row..=merged.last.row.min(merged.first.row + 1_000) {
                for column in merged.first.column..=merged.last.column.min(merged.first.column + 1_000) {
                    filled.push(CellPosition::new(row, column));
                }
            }
        }
    }
    filled
}

fn grow(range: CellRange, cell: CellPosition) -> CellRange {
    CellRange::new(
        CellPosition::new(range.first.row.min(cell.row), range.first.column.min(cell.column)),
        CellPosition::new(range.last.row.max(cell.row), range.last.column.max(cell.column)),
    )
}

fn overlap(a: &CellRange, b: &CellRange) -> bool {
    a.first.row <= b.last.row
        && b.first.row <= a.last.row
        && a.first.column <= b.last.column
        && b.first.column <= a.last.column
}

/// Joins rectangles that overlap (a few loose cells inside a table's rectangle
/// belong to that table), until none do.
fn join_overlapping(ranges: Vec<CellRange>) -> Vec<CellRange> {
    // A sheet of thousands of scattered cells is left as it is: joining is quadratic.
    if ranges.len() > 2_000 {
        return ranges;
    }
    let mut ranges = ranges;
    loop {
        let mut joined_any = false;
        let mut result: Vec<CellRange> = Vec::new();
        for range in ranges {
            match result.iter_mut().find(|kept| overlap(kept, &range)) {
                Some(kept) => {
                    *kept = grow(grow(*kept, range.first), range.last);
                    joined_any = true;
                }
                None => result.push(range),
            }
        }
        ranges = result;
        if !joined_any {
            return ranges;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsers::xlsx::model::{Cell, CellValue, ExcelTable};

    fn sheet_with(cells: &[&str]) -> Sheet {
        let mut sheet = Sheet::default();
        for name in cells {
            sheet.cells.push(Cell {
                position: CellPosition::parse(name).unwrap(),
                value: CellValue::Text(name.to_string()),
                formula: None,
                link: None,
            });
        }
        sheet
    }

    fn ranges(sheet: &Sheet) -> Vec<String> {
        find_tables(sheet).iter().map(|region| region.range.name()).collect()
    }

    #[test]
    fn separate_blocks_become_separate_tables() {
        // A title in A1, then a 2x2 table in C3:D4, then a table to the right in G3:G4.
        let sheet = sheet_with(&["A1", "C3", "D3", "C4", "D4", "G3", "G4"]);
        assert_eq!(ranges(&sheet), ["A1", "C3:D4", "G3:G4"]);
    }

    #[test]
    fn a_header_with_a_gap_still_holds_its_table() {
        // B1 is empty, but B2 connects A and C through the data rows.
        let sheet = sheet_with(&["A1", "C1", "A2", "B2", "C2"]);
        assert_eq!(ranges(&sheet), ["A1:C2"]);
    }

    #[test]
    fn excel_tables_come_first() {
        let mut sheet = sheet_with(&["A1", "B1", "A2", "B2", "C1"]);
        sheet.excel_tables.push(ExcelTable {
            name: "Prices".to_string(),
            range: CellRange::parse("A1:B2").unwrap(),
            header_rows: 1,
        });
        let tables = find_tables(&sheet);
        assert_eq!(tables[0].name.as_deref(), Some("Prices"));
        assert_eq!(tables[1].range.name(), "C1");
    }
}
