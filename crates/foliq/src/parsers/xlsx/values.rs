//! How a cell's value is written as text, and which number formats mean a
//! date or a percentage.
//!
//! foliq writes values plainly, the way an agent can use them: dates as ISO
//! (`2024-01-05 14:30`), numbers without thousands separators, at most 15
//! significant digits (as Excel shows them). Currency and other number
//! formats are not copied.

use super::model::CellValue;

/// The text of a value, as it goes into a table cell.
pub fn value_text(value: &CellValue) -> String {
    match value {
        CellValue::Empty => String::new(),
        CellValue::Text(text) => text.clone(),
        CellValue::Number(number) => number_text(*number),
        CellValue::Percent(number) => format!("{}%", number_text(number * 100.0)),
        CellValue::Bool(true) => "TRUE".to_string(),
        CellValue::Bool(false) => "FALSE".to_string(),
        CellValue::DateTime(serial) => date_time_text(*serial),
        CellValue::Error(error) => error.clone(),
    }
}

/// `30.0` as `30`, `0.1 + 0.2` as `0.3`: rounded to 15 significant digits like Excel.
pub fn number_text(number: f64) -> String {
    if !number.is_finite() {
        return number.to_string();
    }
    if number.fract() == 0.0 && number.abs() < 1e15 {
        return format!("{}", number as i64);
    }
    // `{:.14e}` keeps 15 significant digits; reading it back drops the noise
    // digits, and `{}` then prints the shortest text for that number.
    let rounded: f64 = format!("{number:.14e}").parse().unwrap_or(number);
    format!("{rounded}")
}

/// An Excel date serial (days since 1899-12-30, time as the fraction) as ISO text.
///
/// `45292.0` is `2024-01-01`, `45292.5` is `2024-01-01 12:00`, and a time
/// alone (`0.75`) is `18:00`.
pub fn date_time_text(serial: f64) -> String {
    if !serial.is_finite() || !(0.0..3_000_000.0).contains(&serial) {
        return number_text(serial);
    }
    // Round to the second first, so 0.9999999 becomes the next day, not 23:59:59.
    let total_seconds = (serial * 86_400.0).round() as i64;
    let days = total_seconds.div_euclid(86_400);
    let seconds = total_seconds.rem_euclid(86_400);
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    let time = if seconds != 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{hours:02}:{minutes:02}")
    };
    if days == 0 {
        return time;
    }
    let (year, month, day) = date_from_serial_days(days);
    if hours == 0 && minutes == 0 && seconds == 0 {
        format!("{year:04}-{month:02}-{day:02}")
    } else {
        format!("{year:04}-{month:02}-{day:02} {time}")
    }
}

/// The calendar date of a whole Excel serial.
fn date_from_serial_days(days: i64) -> (i64, i64, i64) {
    // Excel counts 1900-02-29, a day that never existed (serial 60), so the
    // days before it are one off.
    if days == 60 {
        return (1900, 2, 29);
    }
    let days = if days < 60 { days + 1 } else { days };
    // Days since 1970-01-01, then the usual civil-calendar formula
    // (Howard Hinnant's `civil_from_days`).
    let since_1970 = days - 25_569;
    let shifted = since_1970 + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// What a number format makes of a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberKind {
    Plain,
    Percent,
    DateTime,
}

/// The kind of a built-in format id (the formats every Excel knows by number).
pub fn builtin_format_kind(format_id: u32) -> NumberKind {
    match format_id {
        9 | 10 => NumberKind::Percent,
        14..=22 | 27..=36 | 45..=47 | 50..=58 => NumberKind::DateTime,
        _ => NumberKind::Plain,
    }
}

/// The kind of a custom format code like `yyyy-mm-dd`, `0.0%` or `#,##0.00 "€"`.
pub fn format_code_kind(code: &str) -> NumberKind {
    // Only the first section counts (positive numbers): `0.0%;-0.0%`.
    let mut in_quotes = false;
    let mut in_brackets = false;
    let mut previous = ' ';
    for character in code.chars() {
        match character {
            '"' => in_quotes = !in_quotes,
            _ if in_quotes => {}
            // `[Red]`, `[$-409]` and `[h]` are in brackets: only `[h]`-like ones are times.
            '[' => in_brackets = true,
            ']' => in_brackets = false,
            _ if in_brackets => {
                if matches!(character.to_ascii_lowercase(), 'h' | 'm' | 's') && previous == '[' {
                    return NumberKind::DateTime;
                }
            }
            // `\x` and `_x` show the next character as it is.
            _ if previous == '\\' || previous == '_' => {}
            ';' => break,
            '%' => return NumberKind::Percent,
            'y' | 'Y' | 'd' | 'D' | 'h' | 'H' | 's' | 'S' | 'm' | 'M' => return NumberKind::DateTime,
            _ => {}
        }
        previous = character;
    }
    NumberKind::Plain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_plain() {
        assert_eq!(number_text(30.0), "30");
        assert_eq!(number_text(0.1 + 0.2), "0.3");
        assert_eq!(number_text(-2.5), "-2.5");
        assert_eq!(value_text(&CellValue::Percent(0.215)), "21.5%");
    }

    #[test]
    fn dates_are_iso() {
        assert_eq!(date_time_text(45292.0), "2024-01-01");
        assert_eq!(date_time_text(45292.5), "2024-01-01 12:00");
        assert_eq!(date_time_text(0.75), "18:00");
        assert_eq!(date_time_text(1.0), "1900-01-01");
        assert_eq!(date_time_text(61.0), "1900-03-01");
        assert_eq!(date_time_text(25569.0), "1970-01-01");
    }

    #[test]
    fn format_codes_are_recognized() {
        assert_eq!(format_code_kind("yyyy-mm-dd"), NumberKind::DateTime);
        assert_eq!(format_code_kind("[h]:mm"), NumberKind::DateTime);
        assert_eq!(format_code_kind("0.0%"), NumberKind::Percent);
        assert_eq!(format_code_kind("#,##0.00 \"days\""), NumberKind::Plain);
        assert_eq!(format_code_kind("[Red]#,##0"), NumberKind::Plain);
        assert_eq!(format_code_kind("[$-409]General"), NumberKind::Plain);
        assert_eq!(builtin_format_kind(14), NumberKind::DateTime);
    }
}
