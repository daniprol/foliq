//! Groups a sheet's formulas for the `## Formulas` section.
//!
//! A column of totals usually holds one formula filled down 500 rows. Writing
//! it 500 times would bury the data, so each fill-down (or fill-across)
//! becomes one line: `F2:F500 (Total) = D2*E2`. Excel marks most fill-downs
//! itself (shared formulas). Formulas copied one by one are grouped here when
//! they are the same formula relative to their cell (`D2*E2` in row 2 and
//! `D3*E3` in row 3) and sit next to each other.

use std::collections::HashSet;

use super::model::{CellPosition, CellRange, Sheet};

/// One formula and the cells it fills.
#[derive(Debug, Clone, PartialEq)]
pub struct FormulaGroup {
    pub range: CellRange,
    /// The formula of the range's first cell, without `=`.
    pub formula: String,
}

/// The formula groups of a sheet, top to bottom, then left to right.
pub fn formula_groups(sheet: &Sheet) -> Vec<FormulaGroup> {
    let mut groups: Vec<FormulaGroup> = sheet
        .shared_formulas
        .iter()
        .filter(|shared| is_worth_showing(&shared.formula))
        .map(|shared| FormulaGroup {
            range: shared.range,
            formula: shared.formula.clone(),
        })
        .collect();

    // The other formulas, with their relative form, in column order so a fill-down is a run.
    // The other cells of a shared formula carry no formula text; only its first cell does.
    let shared_starts: HashSet<CellPosition> = sheet.shared_formulas.iter().map(|shared| shared.range.first).collect();
    let mut singles: Vec<(CellPosition, String, String)> = Vec::new();
    for cell in &sheet.cells {
        if let Some(formula) = &cell.formula
            && !shared_starts.contains(&cell.position)
            && is_worth_showing(formula)
        {
            singles.push((cell.position, formula.clone(), relative_form(formula, cell.position)));
        }
    }
    singles.sort_by_key(|(position, _, _)| (position.column, position.row));

    let mut runs: Vec<(CellRange, String, String)> = Vec::new();
    for (position, formula, relative) in singles {
        if let Some((range, _, last_relative)) = runs.last_mut()
            && range.last.column == position.column
            && range.last.row + 1 == position.row
            && *last_relative == relative
        {
            range.last = position;
            continue;
        }
        runs.push((CellRange::new(position, position), formula, relative));
    }
    // Single cells side by side in a row (a fill-across) are joined the same way.
    runs.sort_by_key(|(range, _, _)| (range.first.row, range.first.column));
    let mut joined: Vec<(CellRange, String, String)> = Vec::new();
    for (range, formula, relative) in runs {
        if let Some((last_range, _, last_relative)) = joined.last_mut()
            && range.first == range.last
            && last_range.first.row == last_range.last.row
            && last_range.first.row == range.first.row
            && last_range.last.column + 1 == range.first.column
            && *last_relative == relative
        {
            last_range.last = range.last;
            continue;
        }
        joined.push((range, formula, relative));
    }
    groups.extend(
        joined
            .into_iter()
            .map(|(range, formula, _)| FormulaGroup { range, formula }),
    );
    groups.sort_by_key(|group| (group.range.first.row, group.range.first.column));
    groups
}

/// Plain references (`=A1`, `=Sheet2!B3`) and constants say nothing an agent
/// can't see in the values: only real computations are listed.
fn is_worth_showing(formula: &str) -> bool {
    let formula = formula.trim();
    let computes = formula.chars().any(|character| "+-*/^&<>=%".contains(character)) && !is_number(formula);
    formula.contains('(') || computes
}

fn is_number(text: &str) -> bool {
    text.parse::<f64>().is_ok()
}

/// The formula with each relative cell reference written as an offset from
/// `cell`, so the same formula filled down reads the same in every row.
/// `$` references are absolute and kept. Text in quotes is left alone.
///
/// In C2, `A2*$B$1` becomes `R[0]C[-2]*$B$1`.
pub fn relative_form(formula: &str, cell: CellPosition) -> String {
    let characters: Vec<char> = formula.chars().collect();
    let mut result = String::new();
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        if character == '"' {
            // Copy the quoted text up to its closing quote.
            let end = characters[index + 1..]
                .iter()
                .position(|&c| c == '"')
                .map_or(characters.len(), |offset| index + 1 + offset + 1);
            result.extend(&characters[index..end]);
            index = end;
            continue;
        }
        let starts_word = index == 0 || !is_name_character(characters[index - 1]);
        if starts_word && let Some((length, reference)) = read_reference(&characters[index..], cell) {
            result.push_str(&reference);
            index += length;
            continue;
        }
        result.push(character);
        index += 1;
    }
    result
}

fn is_name_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_' || character == '.'
}

/// Reads a cell reference like `B12`, `$B12` or `$B$12` at the start of
/// `text`. Returns its length and its relative form.
fn read_reference(text: &[char], cell: CellPosition) -> Option<(usize, String)> {
    let mut index = 0;
    let column_is_absolute = text.first() == Some(&'$');
    if column_is_absolute {
        index += 1;
    }
    let letters_start = index;
    while index < text.len() && text[index].is_ascii_alphabetic() {
        index += 1;
    }
    let letters: String = text[letters_start..index].iter().collect();
    if letters.is_empty() || letters.len() > 3 {
        return None;
    }
    let row_is_absolute = text.get(index) == Some(&'$');
    if row_is_absolute {
        index += 1;
    }
    let digits_start = index;
    while index < text.len() && text[index].is_ascii_digit() {
        index += 1;
    }
    let digits: String = text[digits_start..index].iter().collect();
    // `LOG10(` is a function and `A1B` a name, not references.
    if digits.is_empty()
        || text
            .get(index)
            .is_some_and(|&next| is_name_character(next) || next == '(')
    {
        return None;
    }
    let position = CellPosition::parse(&format!("{letters}{digits}"))?;
    let row = if row_is_absolute {
        format!("${digits}")
    } else {
        format!("R[{}]", position.row as i64 - cell.row as i64)
    };
    let column = if column_is_absolute {
        format!("${}", letters.to_ascii_uppercase())
    } else {
        format!("C[{}]", position.column as i64 - cell.column as i64)
    };
    Some((index, format!("{row}{column}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsers::xlsx::model::{Cell, CellValue, SharedFormula};

    fn formula_cell(name: &str, formula: &str) -> Cell {
        Cell {
            position: CellPosition::parse(name).unwrap(),
            value: CellValue::Number(1.0),
            formula: Some(formula.to_string()),
            link: None,
        }
    }

    #[test]
    fn relative_form_ignores_the_row() {
        let in_row_2 = relative_form("D2*E2+$B$1", CellPosition::parse("F2").unwrap());
        let in_row_3 = relative_form("D3*E3+$B$1", CellPosition::parse("F3").unwrap());
        assert_eq!(in_row_2, in_row_3);
        assert_eq!(
            relative_form("LOG10(A1)&\"B2\"", CellPosition::new(0, 1)),
            "LOG10(R[0]C[-1])&\"B2\""
        );
    }

    #[test]
    fn fill_downs_become_one_group() {
        let mut sheet = Sheet::default();
        sheet.cells.push(formula_cell("F2", "D2*E2"));
        sheet.cells.push(formula_cell("F3", "D3*E3"));
        sheet.cells.push(formula_cell("F4", "D4*E4"));
        sheet.cells.push(formula_cell("F5", "SUM(F2:F4)"));
        sheet.cells.push(formula_cell("G2", "Sheet2!A1"));
        sheet.shared_formulas.push(SharedFormula {
            range: CellRange::parse("H2:H9").unwrap(),
            formula: "F2*1.21".to_string(),
        });
        let groups: Vec<String> = formula_groups(&sheet)
            .iter()
            .map(|group| format!("{} = {}", group.range.name(), group.formula))
            .collect();
        assert_eq!(groups, ["F2:F4 = D2*E2", "H2:H9 = F2*1.21", "F5 = SUM(F2:F4)"]);
    }
}
