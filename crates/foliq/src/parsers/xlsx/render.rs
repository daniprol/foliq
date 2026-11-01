//! Writes a sheet as Markdown.
//!
//! ```text
//! # Sales                                  one heading per sheet ("(hidden)" if it is)
//! 1200 rows × 6 columns. Tables: ...       a summary, when the sheet has several tables
//! ## Orders (A1:F1201)                     one heading per table, when there are several
//! ### Rows 2–41                            big tables in blocks, each repeating the header
//! | Date | Product | Photo | ... |
//! Chart "Revenue" (line): ...              charts under the table they read from
//! ## Formulas                              one line per formula or fill-down
//! ## Notes                                 comments and text boxes
//! ```
//!
//! A table that fits in one chunk is written whole, and a sheet with one
//! table gets no table heading: a simple sheet stays a heading and a table.

use std::collections::HashMap;

use super::formulas::{FormulaGroup, formula_groups};
use super::layout::{TableRegion, find_tables};
use super::model::{Cell, CellPosition, CellValue, Chart, DefinedName, NoteKind, Picture, Sheet, column_letters};
use super::values::value_text;
use crate::markdown::MarkdownWriter;

/// The most characters of table text in one block of rows. Blocks this size
/// stay under the splitter's default chunk size (400 tokens), so every chunk
/// is a whole table with its header.
const ROW_BLOCK_CHARACTERS: usize = 1000;

/// In a table written as records, column names longer than this are cut.
const MAX_RECORD_NAME: usize = 80;

/// At most this many formula lines per sheet; the rest are counted.
const MAX_FORMULA_LINES: usize = 50;

pub fn write_sheet(writer: &mut MarkdownWriter, sheet: &Sheet) {
    let title = if sheet.is_hidden {
        format!("{} (hidden)", sheet.name)
    } else {
        sheet.name.clone()
    };
    writer.heading(1, &title);

    let regions = find_tables(sheet);
    let pictures = pictures_by_cell(&sheet.pictures);
    let tables: Vec<Table> = regions
        .iter()
        .map(|region| build_table(sheet, region, &pictures))
        .collect();

    // Each chart goes under the table it reads from, or at the end of the sheet.
    let mut charts_after: Vec<Vec<&Chart>> = vec![Vec::new(); tables.len()];
    let mut charts_at_end: Vec<&Chart> = Vec::new();
    for chart in &sheet.charts {
        match chart_table(chart, sheet, &tables) {
            Some(index) => charts_after[index].push(chart),
            None => charts_at_end.push(chart),
        }
    }

    // A sheet with one small table stays a heading and a table. Several tables,
    // or a big one cut into blocks, get a summary and a heading per table.
    let real_tables = tables.iter().filter(|table| !table.is_single_cell()).count();
    let is_big = tables.iter().any(Table::needs_row_blocks);
    let needs_headings = real_tables > 1 || is_big || tables.iter().any(|table| table.name.is_some());
    if real_tables > 1 || is_big {
        writer.paragraph(&summary(sheet, &tables));
    }
    for (table, charts) in tables.iter().zip(&charts_after) {
        write_table(writer, table, needs_headings);
        for chart in charts {
            writer.paragraph(&chart_line(chart));
        }
    }
    for chart in charts_at_end {
        writer.paragraph(&chart_line(chart));
    }

    if sheet.is_cut {
        writer.paragraph("This sheet holds more text than foliq reads (10 MB): its last cells were skipped.");
    }
    write_formulas(writer, &formula_groups(sheet), &tables);
    write_notes(writer, sheet);
}

/// `# Names`: the workbook's defined names, after the last sheet.
pub fn write_names(writer: &mut MarkdownWriter, names: &[DefinedName]) {
    if names.is_empty() {
        return;
    }
    writer.heading(1, "Names");
    for name in names {
        writer.list_item(&format!("{} = {}", name.name, name.formula), 0);
    }
}

/// A table ready to write: its text, cell by cell.
struct Table {
    region: TableRegion,
    name: Option<String>,
    /// A label written above the header, inside the table's rectangle.
    label: Option<String>,
    /// The sheet columns the table kept (columns empty in every row are dropped).
    columns: Vec<u32>,
    /// The header row's text.
    header: Vec<String>,
    /// The other rows, with their row number on the sheet (from 0).
    rows: Vec<(u32, Vec<String>)>,
}

impl Table {
    fn is_single_cell(&self) -> bool {
        self.rows.is_empty() && self.header.len() == 1
    }

    /// `Orders (A1:F1201)`, or just the range for a table with no name or label.
    fn heading(&self) -> String {
        match self.name.as_ref().or(self.label.as_ref()) {
            Some(name) => format!("{name} ({})", self.region.range.name()),
            None => self.region.range.name(),
        }
    }

    /// A header longer than half a block leaves no room for rows: the table
    /// is written as records instead.
    fn is_wide(&self) -> bool {
        row_size(&self.header) > ROW_BLOCK_CHARACTERS / 2
    }

    fn needs_row_blocks(&self) -> bool {
        let size: usize = self.rows.iter().map(|(_, row)| row_size(row)).sum();
        size + row_size(&self.header) > ROW_BLOCK_CHARACTERS
    }
}

/// About how many characters a row takes as a Markdown table line.
fn row_size(row: &[String]) -> usize {
    row.iter().map(|cell| cell.len() + 3).sum::<usize>() + 2
}

fn build_table(sheet: &Sheet, region: &TableRegion, pictures: &HashMap<CellPosition, Vec<&Picture>>) -> Table {
    let range = region.range;
    let header_rows = region.header_rows.max(1).min(range.row_count());
    let width = range.column_count() as usize;

    // First the plain text of every cell, so pictures can borrow a neighbour's words.
    // The cells are sorted row by row: jump to the table's first row, read to its last.
    let mut grid: Vec<(u32, Vec<String>)> = Vec::new();
    let first_index = sheet.cells.partition_point(|cell| cell.position.row < range.first.row);
    for cell in &sheet.cells[first_index..] {
        if cell.position.row > range.last.row {
            break;
        }
        if !range.contains(cell.position) {
            continue;
        }
        let text = cell_text(cell);
        if text.is_empty() {
            continue;
        }
        if grid.last().is_none_or(|(row, _)| *row != cell.position.row) {
            grid.push((cell.position.row, vec![String::new(); width]));
        }
        let (_, texts) = grid.last_mut().expect("just pushed");
        texts[(cell.position.column - range.first.column) as usize] = text;
    }
    // Rows that hold only a picture are rows too.
    for anchor in pictures.keys().filter(|anchor| range.contains(**anchor)) {
        if let Err(index) = grid.binary_search_by_key(&anchor.row, |(row, _)| *row) {
            grid.insert(index, (anchor.row, vec![String::new(); width]));
        }
    }

    // A first row with a single label over a wider table ("Reading List" above
    // the columns) is the table's title, and the header is the row below it.
    let mut label = None;
    let first_row_cells = grid
        .first()
        .map_or(0, |(_, texts)| texts.iter().filter(|text| !text.is_empty()).count());
    if region.name.is_none() && first_row_cells == 1 && grid.len() >= 3 && range.column_count() >= 2 {
        let (_, texts) = grid.remove(0);
        label = texts.into_iter().find(|text| !text.is_empty());
    }
    let header_start = grid.first().map_or(range.first.row, |(row, _)| *row);

    // A merged header cell names every column it spans.
    for merged in &sheet.merged_ranges {
        let in_header = merged.first.row >= header_start && merged.first.row < header_start + header_rows;
        if !in_header || !range.contains(merged.first) {
            continue;
        }
        if let Some((_, texts)) = grid.iter_mut().find(|(row, _)| *row == merged.first.row) {
            let first = (merged.first.column - range.first.column) as usize;
            let last = (merged.last.column.min(range.last.column) - range.first.column) as usize;
            let value = texts[first].clone();
            for text in &mut texts[first..=last] {
                if text.is_empty() {
                    *text = value.clone();
                }
            }
        }
    }

    // Then the pictures, described by their alt text or the words beside them.
    let header_texts: Vec<String> = grid.first().map(|(_, texts)| texts.clone()).unwrap_or_default();
    let plain_grid = grid.clone();
    for (row_index, (row, texts)) in grid.iter_mut().enumerate() {
        for (column_index, text) in texts.iter_mut().enumerate() {
            let position = CellPosition::new(*row, range.first.column + column_index as u32);
            for picture in pictures.get(&position).into_iter().flatten() {
                let row_texts = &plain_grid[row_index].1;
                let description = picture
                    .description
                    .clone()
                    .or_else(|| nearest_text(row_texts, column_index))
                    .or_else(|| {
                        header_texts
                            .get(column_index)
                            .filter(|text| !text.is_empty() && row_index > 0)
                            .cloned()
                    })
                    .unwrap_or_default();
                let image = format!("![{}]({})", clean_alt_text(&description), picture.part_path);
                *text = if text.is_empty() {
                    image
                } else {
                    format!("{text} {image}")
                };
            }
        }
    }

    // Columns empty in every row of the table are dropped.
    let used: Vec<usize> = (0..width)
        .filter(|&column| grid.iter().any(|(_, texts)| !texts[column].is_empty()))
        .collect();
    let mut rows: Vec<(u32, Vec<String>)> = grid
        .into_iter()
        .map(|(row, texts)| (row, used.iter().map(|&column| texts[column].clone()).collect()))
        .collect();

    // Several header rows are joined into one, column by column.
    let header_count = rows
        .iter()
        .take_while(|(row, _)| *row < header_start + header_rows)
        .count()
        .max(1)
        .min(rows.len());
    let header_lines: Vec<(u32, Vec<String>)> = rows.drain(..header_count).collect();
    let header: Vec<String> = (0..used.len())
        .map(|column| {
            let mut words: Vec<String> = Vec::new();
            for (_, line) in &header_lines {
                let text = &line[column];
                if !text.is_empty() && !words.contains(text) {
                    words.push(text.clone());
                }
            }
            words.join(" / ")
        })
        .collect();

    Table {
        region: region.clone(),
        name: region.name.clone(),
        label,
        columns: used.iter().map(|&column| range.first.column + column as u32).collect(),
        header,
        rows,
    }
}

/// What a cell shows: its value (with its link), or its formula if it was never calculated.
fn cell_text(cell: &Cell) -> String {
    let text = match (&cell.value, &cell.formula) {
        (CellValue::Empty, Some(formula)) => format!("={formula}"),
        (value, _) => value_text(value),
    };
    match &cell.link {
        Some(link) if !text.is_empty() => format!("[{}]({})", text.replace(['[', ']'], ""), link_target(link)),
        Some(link) => format!("<{link}>"),
        None => text,
    }
}

/// A link target that stays one Markdown link even with spaces or parentheses.
fn link_target(link: &str) -> String {
    if link.contains([' ', '(', ')']) {
        format!("<{link}>")
    } else {
        link.to_string()
    }
}

fn clean_alt_text(text: &str) -> String {
    text.replace(['[', ']'], "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The words next to a picture: the nearest text to its right, else to its left.
fn nearest_text(row: &[String], column: usize) -> Option<String> {
    let is_words = |text: &&String| !text.is_empty() && !text.starts_with("![");
    row[column + 1..]
        .iter()
        .find(is_words)
        .or_else(|| row[..column].iter().rev().find(is_words))
        .cloned()
}

fn pictures_by_cell(pictures: &[Picture]) -> HashMap<CellPosition, Vec<&Picture>> {
    let mut by_cell: HashMap<CellPosition, Vec<&Picture>> = HashMap::new();
    for picture in pictures {
        by_cell.entry(picture.anchor).or_default().push(picture);
    }
    by_cell
}

fn write_table(writer: &mut MarkdownWriter, table: &Table, with_heading: bool) {
    // A lone cell (a title, a note) is a paragraph, not a one-cell table.
    if table.is_single_cell() {
        writer.paragraph(&table.header[0]);
        return;
    }
    if with_heading {
        writer.heading(2, &table.heading());
    } else if let Some(label) = &table.label {
        writer.paragraph(label);
    }
    if table.is_wide() {
        write_records(writer, table);
        return;
    }
    if !table.needs_row_blocks() {
        let mut lines = vec![table.header.clone()];
        lines.extend(table.rows.iter().map(|(_, row)| row.clone()));
        writer.table(&lines);
        return;
    }
    let header_size = row_size(&table.header);
    for (start, end) in row_blocks(&table.rows, header_size, row_size) {
        writer.heading(3, &rows_title(&table.rows[start..end]));
        let mut lines = vec![table.header.clone()];
        lines.extend(table.rows[start..end].iter().map(|(_, row)| row.clone()));
        writer.table(&lines);
    }
}

/// A table too wide to read as a grid: each row becomes one line of
/// `column: value` pairs, leaving out the empty cells.
///
/// `- Row 7: Project ID: P-104; Status: Active; Budget: 120000`
fn write_records(writer: &mut MarkdownWriter, table: &Table) {
    let record = |row_number: u32, row: &[String]| -> String {
        let pairs: Vec<String> = row
            .iter()
            .enumerate()
            .filter(|(_, value)| !value.is_empty())
            .map(|(index, value)| {
                // A header is a short name; a very long one is cut so it doesn't repeat on every row.
                let name = match table.header[index].as_str() {
                    "" => column_letters(table.columns[index]),
                    header if header.chars().count() > MAX_RECORD_NAME => {
                        format!("{}…", header.chars().take(MAX_RECORD_NAME).collect::<String>())
                    }
                    header => header.to_string(),
                };
                format!("{name}: {value}")
            })
            .collect();
        format!("Row {}: {}", row_number + 1, pairs.join("; "))
    };
    let records: Vec<(u32, Vec<String>)> = table
        .rows
        .iter()
        .map(|(row_number, row)| (*row_number, vec![record(*row_number, row)]))
        .collect();
    let blocks = row_blocks(&records, 0, |line| line[0].len() + 3);
    let needs_titles = blocks.len() > 1;
    for (start, end) in blocks {
        if needs_titles {
            writer.heading(3, &rows_title(&records[start..end]));
        }
        for (_, line) in &records[start..end] {
            writer.list_item(&line[0], 0);
        }
    }
}

/// Cuts rows into blocks of at most [`ROW_BLOCK_CHARACTERS`] (at least one row each).
/// Returns `(start, end)` index pairs, like Python's `rows[start:end]`.
fn row_blocks(
    rows: &[(u32, Vec<String>)],
    fixed_size: usize,
    size_of: impl Fn(&[String]) -> usize,
) -> Vec<(usize, usize)> {
    let mut blocks = Vec::new();
    let mut start = 0;
    while start < rows.len() {
        let mut end = start;
        let mut size = fixed_size;
        while end < rows.len() && (end == start || size + size_of(&rows[end].1) <= ROW_BLOCK_CHARACTERS) {
            size += size_of(&rows[end].1);
            end += 1;
        }
        blocks.push((start, end));
        start = end;
    }
    blocks
}

/// `Rows 2–41`, with the row numbers Excel shows.
fn rows_title(rows: &[(u32, Vec<String>)]) -> String {
    let first = rows.first().map_or(0, |(row, _)| row + 1);
    let last = rows.last().map_or(0, |(row, _)| row + 1);
    if first == last {
        format!("Row {first}")
    } else {
        format!("Rows {first}–{last}")
    }
}

/// `1200 rows × 6 columns. Tables: Orders (A1:F1201), H1:I4. Charts: Revenue by month.`
fn summary(sheet: &Sheet, tables: &[Table]) -> String {
    let rows = sheet.cells.iter().map(|cell| cell.position.row).max().unwrap_or(0)
        - sheet.cells.iter().map(|cell| cell.position.row).min().unwrap_or(0)
        + 1;
    let columns = sheet.cells.iter().map(|cell| cell.position.column).max().unwrap_or(0)
        - sheet.cells.iter().map(|cell| cell.position.column).min().unwrap_or(0)
        + 1;
    let mut text = format!("{rows} rows × {columns} columns.");
    let names: Vec<String> = tables
        .iter()
        .filter(|table| !table.is_single_cell())
        .map(Table::heading)
        .collect();
    if !names.is_empty() {
        text.push_str(&format!(" Tables: {}.", names.join(", ")));
    }
    let charts: Vec<String> = sheet.charts.iter().filter_map(|chart| chart.title.clone()).collect();
    if !charts.is_empty() {
        text.push_str(&format!(" Charts: {}.", charts.join(", ")));
    }
    text
}

/// The table a chart reads its values from (on this sheet), if any.
fn chart_table(chart: &Chart, sheet: &Sheet, tables: &[Table]) -> Option<usize> {
    let values = chart.series.first()?.values.as_deref()?;
    let (sheet_name, cells) = match values.rsplit_once('!') {
        Some((sheet_name, cells)) => (Some(sheet_name.trim_matches('\'')), cells),
        None => (None, values),
    };
    if sheet_name.is_some_and(|name| name != sheet.name) {
        return None;
    }
    let first = CellPosition::parse(cells.split(':').next()?)?;
    tables.iter().position(|table| table.region.range.contains(first))
}

/// `Chart "Revenue by month" (line): Revenue (Sales!F2:F13); labels Sales!A2:A13.`
fn chart_line(chart: &Chart) -> String {
    let mut text = match &chart.title {
        Some(title) => format!("Chart \"{title}\" ({})", chart.kind),
        None => format!("Chart ({})", chart.kind),
    };
    let series: Vec<String> = chart
        .series
        .iter()
        .enumerate()
        .map(|(index, series)| {
            let name = series.name.clone().unwrap_or_else(|| format!("series {}", index + 1));
            match &series.values {
                Some(values) => format!("{name} ({values})"),
                None => name,
            }
        })
        .collect();
    if !series.is_empty() {
        text.push_str(&format!(": {}", series.join(", ")));
    }
    if let Some(labels) = chart.series.iter().find_map(|series| series.categories.clone()) {
        text.push_str(&format!("; labels {labels}"));
    }
    text.push('.');
    text
}

/// `## Formulas`: `F2:F1201 (Total) = D2*E2`, with the column's header name.
fn write_formulas(writer: &mut MarkdownWriter, groups: &[FormulaGroup], tables: &[Table]) {
    if groups.is_empty() {
        return;
    }
    writer.heading(2, "Formulas");
    for group in groups.iter().take(MAX_FORMULA_LINES) {
        let header = column_header(group.range.first, tables);
        let line = match header {
            Some(header) => format!("{} ({header}) = {}", group.range.name(), group.formula),
            None => format!("{} = {}", group.range.name(), group.formula),
        };
        writer.list_item(&line, 0);
    }
    if groups.len() > MAX_FORMULA_LINES {
        writer.list_item(&format!("… and {} more", groups.len() - MAX_FORMULA_LINES), 0);
    }
}

/// The header text above a cell, if the cell is in a table's body.
fn column_header(cell: CellPosition, tables: &[Table]) -> Option<String> {
    let table = tables
        .iter()
        .find(|table| table.region.range.contains(cell) && cell.row > table.region.range.first.row)?;
    let position = table.columns.iter().position(|&column| column == cell.column)?;
    table.header.get(position).filter(|text| !text.is_empty()).cloned()
}

/// `## Notes`: comments with their replies, and text boxes.
fn write_notes(writer: &mut MarkdownWriter, sheet: &Sheet) {
    if sheet.notes.is_empty() {
        return;
    }
    writer.heading(2, "Notes");
    for note in &sheet.notes {
        let cell = note.cell.name();
        let line = match note.kind {
            NoteKind::TextBox => format!("Text box at {cell}: {}", note.text),
            NoteKind::Comment => {
                let who: Vec<&str> = [note.author.as_deref(), note.date.as_deref()]
                    .into_iter()
                    .flatten()
                    .collect();
                if who.is_empty() {
                    format!("{cell}: {}", note.text)
                } else {
                    format!("{cell} ({}): {}", who.join(", "), note.text)
                }
            }
        };
        writer.list_item(&line, 0);
        for reply in &note.replies {
            let line = match &reply.author {
                Some(author) => format!("{author}: {}", reply.text),
                None => reply.text.clone(),
            };
            writer.list_item(&line, 1);
        }
    }
}
