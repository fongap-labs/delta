//! Tables as data: line two spreadsheets or CSV files up on a key and say what differs, and check a
//! ledger for repeated rows, sums that do not add up and dates that cannot be right.
//!
//! Everything here is arithmetic and comparison on the cells as they are in the file, with no model
//! in the loop: the tools return the rows they found and how many there are in all, so a figure
//! the model reports about a table is one the tool computed (ADR-0088).

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;

use serde_json::{json, Value};

use super::ooxml;
use crate::capability::{
    CapabilityGrants, CapabilityRegistration, CapabilityResult, NativeCapabilityRunner,
};

/// Biggest table read, in bytes of the file.
const MAX_TABLE_BYTES: u64 = 50 * 1024 * 1024;
/// How many rows from the top are looked at to find the heading row.
const HEADER_SEARCH_ROWS: usize = 10;
/// Findings of each kind listed in an answer; the counts are always complete.
const MAX_LISTED: usize = 100;

/// One row of a table, with the row number it has in the file (the sheet's own, or the line of a
/// CSV file counted from 1 with the header as line 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub number: usize,
    pub cells: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub name: String,
    pub header: Vec<String>,
    pub rows: Vec<Row>,
}

impl Table {
    /// The position of a column, by its heading (ignoring case and surrounding space).
    pub fn column(&self, name: &str) -> Option<usize> {
        let wanted = name.trim().to_lowercase();
        self.header
            .iter()
            .position(|heading| heading.trim().to_lowercase() == wanted)
    }

    fn cell<'a>(&self, row: &'a Row, column: usize) -> &'a str {
        row.cells.get(column).map_or("", |cell| cell.trim())
    }
}

/// Read a table from a CSV, TSV or XLSX file. `sheet` picks a worksheet by name (the first one by
/// default). The header is the first row with content, or row `header_row` when given.
pub fn load_table(
    path: &Path,
    sheet: Option<&str>,
    header_row: Option<usize>,
) -> Result<Table, String> {
    let size = std::fs::metadata(path)
        .map_err(|error| format!("could not read the file: {error}"))?
        .len();
    if size > MAX_TABLE_BYTES {
        return Err(format!(
            "the file is too large to read as a table ({size} bytes)"
        ));
    }
    let bytes = std::fs::read(path).map_err(|error| format!("could not read the file: {error}"))?;
    let extension = path
        .extension()
        .and_then(|found| found.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let name = path
        .file_name()
        .map(|file| file.to_string_lossy().to_string())
        .unwrap_or_default();
    match extension.as_str() {
        "csv" | "tsv" | "txt" => table_from_text(&name, &bytes, extension == "tsv", header_row),
        "xlsx" | "xlsm" => table_from_xlsx(&name, &bytes, sheet, header_row),
        _ => Err(format!(
            "a table is read from a .csv, .tsv or .xlsx file, not .{extension}"
        )),
    }
}

fn table_from_text(
    name: &str,
    bytes: &[u8],
    is_tab_separated: bool,
    header_row: Option<usize>,
) -> Result<Table, String> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim_start_matches('\u{feff}');
    let first_line = text.lines().next().unwrap_or("");
    let delimiter = if is_tab_separated {
        b'\t'
    } else if first_line.matches(';').count() > first_line.matches(',').count() {
        b';'
    } else {
        b','
    };
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter)
        .from_reader(text.as_bytes());
    let mut records: Vec<Row> = Vec::new();
    for (index, record) in reader.records().enumerate() {
        let record = record.map_err(|error| format!("{name} is not a valid table: {error}"))?;
        // The line the record starts on, counted from the bytes before it, so a blank line or a
        // quoted line break does not shift the numbers away from the ones the file's own editor
        // shows.
        let line = record.position().map_or(index + 1, |position| {
            let bytes = text.as_bytes();
            let at = (position.byte() as usize).min(bytes.len());
            // The reader reports where it began looking, which is before any blank lines it
            // skipped on the way to the record.
            let blank_lines = bytes[at..]
                .iter()
                .take_while(|byte| matches!(**byte, b'\n' | b'\r'))
                .filter(|byte| **byte == b'\n')
                .count();
            bytes[..at].iter().filter(|byte| **byte == b'\n').count() + blank_lines + 1
        });
        records.push(Row {
            number: line,
            cells: record.iter().map(str::to_owned).collect(),
        });
    }
    table_from_rows(name, records, header_row)
}

fn table_from_xlsx(
    name: &str,
    bytes: &[u8],
    sheet: Option<&str>,
    header_row: Option<usize>,
) -> Result<Table, String> {
    let sheets = ooxml::read_xlsx(bytes)?;
    let chosen = match sheet {
        Some(wanted) => sheets
            .iter()
            .find(|candidate| candidate.name.trim().eq_ignore_ascii_case(wanted.trim()))
            .ok_or_else(|| {
                format!(
                    "{name} has no sheet {wanted:?}; its sheets are {}",
                    sheets
                        .iter()
                        .map(|candidate| format!("{:?}", candidate.name))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })?,
        None => sheets
            .first()
            .ok_or_else(|| format!("{name} has no sheets"))?,
    };
    let rows = chosen
        .rows
        .iter()
        .map(|(number, cells)| Row {
            number: *number,
            cells: cells.clone(),
        })
        .collect();
    let mut table = table_from_rows(name, rows, header_row)?;
    table.name = format!("{name} / {}", chosen.name);
    Ok(table)
}

fn table_from_rows(name: &str, rows: Vec<Row>, header_row: Option<usize>) -> Result<Table, String> {
    let is_blank = |row: &Row| row.cells.iter().all(|cell| cell.trim().is_empty());
    let header_at = match header_row {
        Some(wanted) => rows
            .iter()
            .position(|row| row.number == wanted)
            .ok_or_else(|| format!("{name} has no row {wanted} to use as the header"))?,
        // A report usually opens with a title row (often one merged cell) above the headings, so
        // the heading row is the first with about as many filled cells as the widest of the first
        // few rows, not simply the first row with anything in it.
        None => {
            let filled = |row: &Row| {
                row.cells
                    .iter()
                    .filter(|cell| !cell.trim().is_empty())
                    .count()
            };
            let widest = rows
                .iter()
                .filter(|row| !is_blank(row))
                .take(HEADER_SEARCH_ROWS)
                .map(filled)
                .max()
                .ok_or_else(|| format!("{name} has no rows with content"))?;
            rows.iter()
                .position(|row| {
                    !is_blank(row) && (widest < 2 || filled(row) * 2 >= widest && filled(row) >= 2)
                })
                .ok_or_else(|| format!("{name} has no rows with content"))?
        }
    };
    let header: Vec<String> = rows[header_at]
        .cells
        .iter()
        .map(|cell| cell.trim().to_owned())
        .collect();
    Ok(Table {
        name: name.to_owned(),
        header,
        rows: rows
            .into_iter()
            .skip(header_at + 1)
            .filter(|row| !is_blank(row))
            .collect(),
    })
}

// -- numbers and keys -------------------------------------------------------------------------

/// A cell as a number: plain digits, a thousands separator, a currency sign, a percent sign or an
/// accounting negative ("(12.50)").
pub fn parse_number(cell: &str) -> Option<f64> {
    let mut text = cell.trim().to_owned();
    if text.is_empty() {
        return None;
    }
    let negative = text.starts_with('(') && text.ends_with(')');
    if negative {
        text = text[1..text.len() - 1].to_owned();
    }
    let is_percent = text.ends_with('%');
    let cleaned: String = text
        .chars()
        .filter(|c| !matches!(c, ',' | ' ' | '\u{a0}' | '%' | '¥' | '$' | '€' | '£' | '￥'))
        .collect();
    let value: f64 = cleaned
        .parse()
        .ok()
        .filter(|value: &f64| value.is_finite())?;
    let value = if is_percent { value / 100.0 } else { value };
    Some(if negative { -value } else { value })
}

/// A key as it is compared: numbers by value ("001" and "1.0" are the key 1), anything else with
/// its case and spacing set aside.
fn normalize_key(cell: &str) -> String {
    let trimmed = cell.trim();
    match trimmed.parse::<f64>() {
        Ok(value) if value.is_finite() && !trimmed.is_empty() => format!("{value}"),
        _ => trimmed
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase(),
    }
}

/// Whether two cells hold the same thing: the same number within `tolerance`, or the same text with
/// its spacing set aside.
fn same_value(left: &str, right: &str, tolerance: f64) -> bool {
    match (parse_number(left), parse_number(right)) {
        (Some(a), Some(b)) => (a - b).abs() <= tolerance,
        _ => {
            let words = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
            words(left) == words(right)
        }
    }
}

// -- reconcile --------------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct ReconcileOptions {
    /// The columns that identify a row; both tables must have all of them.
    pub keys: Vec<String>,
    /// The columns to compare. By default every column the two tables share, except the keys.
    pub compare: Option<Vec<String>>,
    /// How far two numbers may differ and still count as the same.
    pub tolerance: f64,
}

fn key_of(table: &Table, row: &Row, columns: &[usize]) -> String {
    columns
        .iter()
        .map(|column| normalize_key(table.cell(row, *column)))
        .collect::<Vec<_>>()
        .join("\u{1f}")
}

fn key_text(table: &Table, row: &Row, columns: &[usize]) -> String {
    columns
        .iter()
        .map(|column| table.cell(row, *column).to_owned())
        .collect::<Vec<_>>()
        .join(" / ")
}

fn indexes(table: &Table, names: &[String], side: &str) -> Result<Vec<usize>, String> {
    names
        .iter()
        .map(|name| {
            table.column(name).ok_or_else(|| {
                format!(
                    "the {side} table has no column {name:?}; its columns are {}",
                    table
                        .header
                        .iter()
                        .map(|heading| format!("{heading:?}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
        })
        .collect()
}

/// Line two tables up on their key and report what only one has, what is repeated, and the cells
/// that differ in the rows both have.
pub fn reconcile(left: &Table, right: &Table, options: &ReconcileOptions) -> Result<Value, String> {
    if options.keys.is_empty() {
        return Err("keys must name at least one column".to_owned());
    }
    let left_keys = indexes(left, &options.keys, "left")?;
    let right_keys = indexes(right, &options.keys, "right")?;
    let compare: Vec<String> = match &options.compare {
        Some(columns) => columns.clone(),
        None => left
            .header
            .iter()
            .filter(|heading| {
                !heading.trim().is_empty()
                    && right.column(heading).is_some()
                    && !options
                        .keys
                        .iter()
                        .any(|key| key.trim().eq_ignore_ascii_case(heading.trim()))
            })
            .cloned()
            .collect(),
    };
    let left_compare = indexes(left, &compare, "left")?;
    let right_compare = indexes(right, &compare, "right")?;

    let mut right_by_key: HashMap<String, Vec<&Row>> = HashMap::new();
    for row in &right.rows {
        right_by_key
            .entry(key_of(right, row, &right_keys))
            .or_default()
            .push(row);
    }
    let mut left_seen: HashMap<String, Vec<&Row>> = HashMap::new();
    for row in &left.rows {
        left_seen
            .entry(key_of(left, row, &left_keys))
            .or_default()
            .push(row);
    }

    let mut only_left = Vec::new();
    let mut differences = Vec::new();
    let mut matched = 0usize;
    let mut differing_rows = 0usize;
    let mut difference_count = 0usize;
    for row in &left.rows {
        let key = key_of(left, row, &left_keys);
        let Some(candidates) = right_by_key.get(&key) else {
            only_left.push(json!({"key": key_text(left, row, &left_keys), "row": row.number}));
            continue;
        };
        // A repeated key is reported below; the first row of each side stands for it here.
        if left_seen[&key][0].number != row.number {
            continue;
        }
        let other = candidates[0];
        matched += 1;
        let mut has_difference = false;
        for ((name, l), r) in compare.iter().zip(&left_compare).zip(&right_compare) {
            let (a, b) = (left.cell(row, *l), right.cell(other, *r));
            if !same_value(a, b, options.tolerance) {
                has_difference = true;
                difference_count += 1;
                let mut entry = json!({
                    "key": key_text(left, row, &left_keys), "column": name,
                    "left": a, "right": b, "left_row": row.number, "right_row": other.number,
                });
                if let (Some(x), Some(y)) = (parse_number(a), parse_number(b)) {
                    entry["difference"] = json!(round6(x - y));
                }
                differences.push(entry);
            }
        }
        if has_difference {
            differing_rows += 1;
        }
    }
    let only_right: Vec<Value> = right
        .rows
        .iter()
        .filter(|row| !left_seen.contains_key(&key_of(right, row, &right_keys)))
        .map(|row| json!({"key": key_text(right, row, &right_keys), "row": row.number}))
        .collect();

    let repeated = |table: &Table, columns: &[usize]| -> Vec<Value> {
        let mut groups: BTreeMap<String, Vec<&Row>> = BTreeMap::new();
        for row in &table.rows {
            groups
                .entry(key_of(table, row, columns))
                .or_default()
                .push(row);
        }
        let mut out: Vec<(usize, Value)> = groups
            .into_values()
            .filter(|rows| rows.len() > 1)
            .map(|rows| {
                (
                    rows[0].number,
                    json!({
                        "key": key_text(table, rows[0], columns),
                        "rows": rows.iter().map(|row| row.number).collect::<Vec<_>>(),
                    }),
                )
            })
            .collect();
        out.sort_by_key(|(first, _)| *first);
        out.into_iter().map(|(_, value)| value).collect()
    };
    let repeated_left = repeated(left, &left_keys);
    let repeated_right = repeated(right, &right_keys);

    let counts = json!({
        "left_rows": left.rows.len(), "right_rows": right.rows.len(),
        "matched": matched,
        "only_in_left": only_left.len(), "only_in_right": only_right.len(),
        "rows_with_differences": differing_rows, "differences": difference_count,
        "repeated_keys_left": repeated_left.len(), "repeated_keys_right": repeated_right.len(),
    });
    let clean = only_left.is_empty()
        && only_right.is_empty()
        && differences.is_empty()
        && repeated_left.is_empty()
        && repeated_right.is_empty();
    Ok(json!({
        "ok": true, "reconciled": clean,
        "left": left.name, "right": right.name,
        "keys": options.keys, "compared_columns": compare, "tolerance": options.tolerance,
        "counts": counts,
        "only_in_left": listed(only_left), "only_in_right": listed(only_right),
        "differences": listed(differences),
        "repeated_keys_left": listed(repeated_left), "repeated_keys_right": listed(repeated_right),
        "note": "Rows with a repeated key are matched on their first row; the repeats are listed so none is lost.",
    }))
}

fn round6(value: f64) -> f64 {
    (value * 1e6).round() / 1e6
}

fn listed(mut findings: Vec<Value>) -> Vec<Value> {
    findings.truncate(MAX_LISTED);
    findings
}

// -- ledger check -----------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct CheckOptions {
    /// Columns that together identify a row that may appear only once (an invoice number and vendor).
    pub duplicates_by: Vec<String>,
    /// Sums that must hold on every row, such as `amount = quantity * unit_price`.
    pub rules: Vec<String>,
    /// Columns that hold dates.
    pub date_columns: Vec<String>,
    /// The earliest and the latest a date may be, as `YYYY-MM-DD`.
    pub not_before: Option<String>,
    pub not_after: Option<String>,
    /// Columns that may not be empty.
    pub required: Vec<String>,
    /// How far a sum may be out and still hold.
    pub tolerance: f64,
}

/// A date as (year, month, day) from the forms people write: 2026-10-28, 2026/10/28, 2026.10.28,
/// a Chinese long-form date, 20261028, or a spreadsheet's day count.
pub fn parse_date(cell: &str) -> Option<(i32, u32, u32)> {
    let text = cell.trim();
    if text.is_empty() {
        return None;
    }
    let digits: Vec<u32> = text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect();
    let compact = text.chars().all(|c| c.is_ascii_digit());
    let (year, month, day) = if compact && text.len() == 8 {
        (
            text[..4].parse().ok()?,
            text[4..6].parse().ok()?,
            text[6..].parse().ok()?,
        )
    } else if let Some(serial) = parse_number(text).filter(|_| compact || text.contains('.')) {
        // A spreadsheet stores a date as the days since 1899-12-30.
        if !(20_000.0..=80_000.0).contains(&serial) {
            return None;
        }
        let base = chrono::NaiveDate::from_ymd_opt(1899, 12, 30)?;
        let date = base.checked_add_signed(chrono::Duration::days(serial.trunc() as i64))?;
        use chrono::Datelike;
        (date.year(), date.month(), date.day())
    } else if digits.len() == 3 && digits[0] >= 1000 {
        (digits[0] as i32, digits[1], digits[2])
    } else {
        return None;
    };
    chrono::NaiveDate::from_ymd_opt(year, month, day)?;
    Some((year, month, day))
}

fn date_text((year, month, day): (i32, u32, u32)) -> String {
    format!("{year:04}-{month:02}-{day:02}")
}

// A sum such as `amount = quantity * unit_price + tax`: columns (bare, or in backticks when the
// heading has spaces), numbers, + - * / and brackets.
#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Number(f64),
    Column(String),
    Binary(Box<Expr>, char, Box<Expr>),
}

struct ExprParser {
    chars: Vec<char>,
    at: usize,
}

impl ExprParser {
    fn skip(&mut self) {
        while self.chars.get(self.at).is_some_and(|c| c.is_whitespace()) {
            self.at += 1;
        }
    }
    fn peek(&mut self) -> Option<char> {
        self.skip();
        self.chars.get(self.at).copied()
    }
    fn expression(&mut self) -> Result<Expr, String> {
        let mut left = self.term()?;
        while let Some(op @ ('+' | '-')) = self.peek() {
            self.at += 1;
            left = Expr::Binary(Box::new(left), op, Box::new(self.term()?));
        }
        Ok(left)
    }
    fn term(&mut self) -> Result<Expr, String> {
        let mut left = self.factor()?;
        while let Some(op @ ('*' | '/')) = self.peek() {
            self.at += 1;
            left = Expr::Binary(Box::new(left), op, Box::new(self.factor()?));
        }
        Ok(left)
    }
    fn factor(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Some('(') => {
                self.at += 1;
                let inner = self.expression()?;
                if self.peek() != Some(')') {
                    return Err("a bracket is not closed".to_owned());
                }
                self.at += 1;
                Ok(inner)
            }
            Some('`') => {
                self.at += 1;
                let start = self.at;
                while self.chars.get(self.at).is_some_and(|c| *c != '`') {
                    self.at += 1;
                }
                if self.chars.get(self.at).is_none() {
                    return Err("a ` is not closed".to_owned());
                }
                let name: String = self.chars[start..self.at].iter().collect();
                self.at += 1;
                Ok(Expr::Column(name))
            }
            Some(c) if c.is_ascii_digit() || c == '.' => {
                let start = self.at;
                while self
                    .chars
                    .get(self.at)
                    .is_some_and(|c| c.is_ascii_digit() || *c == '.')
                {
                    self.at += 1;
                }
                let text: String = self.chars[start..self.at].iter().collect();
                text.parse()
                    .map(Expr::Number)
                    .map_err(|_| format!("{text} is not a number"))
            }
            Some(c) if c.is_alphanumeric() || c == '_' => {
                let start = self.at;
                while self
                    .chars
                    .get(self.at)
                    .is_some_and(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
                {
                    self.at += 1;
                }
                Ok(Expr::Column(self.chars[start..self.at].iter().collect()))
            }
            Some(other) => Err(format!("unexpected {other:?}")),
            None => Err("the sum ends too soon".to_owned()),
        }
    }
}

/// `column = expression`.
fn parse_rule(rule: &str) -> Result<(String, Expr), String> {
    let (target, sum) = rule
        .split_once('=')
        .ok_or_else(|| format!("{rule:?} is not a sum of the form column = expression"))?;
    let target = target.trim().trim_matches('`').trim().to_owned();
    if target.is_empty() {
        return Err(format!("{rule:?} names no column before ="));
    }
    let mut parser = ExprParser {
        chars: sum.chars().collect(),
        at: 0,
    };
    let expression = parser
        .expression()
        .map_err(|error| format!("{rule:?}: {error}"))?;
    if parser.peek().is_some() {
        return Err(format!(
            "{rule:?}: there is something after the end of the sum"
        ));
    }
    Ok((target, expression))
}

fn columns_of(expression: &Expr, out: &mut Vec<String>) {
    match expression {
        Expr::Column(name) => out.push(name.clone()),
        Expr::Binary(left, _, right) => {
            columns_of(left, out);
            columns_of(right, out);
        }
        Expr::Number(_) => {}
    }
}

fn evaluate(expression: &Expr, value_of: &dyn Fn(&str) -> Option<f64>) -> Option<f64> {
    match expression {
        Expr::Number(number) => Some(*number),
        Expr::Column(name) => value_of(name),
        Expr::Binary(left, op, right) => {
            let (a, b) = (evaluate(left, value_of)?, evaluate(right, value_of)?);
            match op {
                '+' => Some(a + b),
                '-' => Some(a - b),
                '*' => Some(a * b),
                '/' => (b != 0.0).then(|| a / b),
                _ => None,
            }
        }
    }
}

/// Check one table for repeated rows, sums that do not hold, dates that cannot be right and
/// required cells left empty.
pub fn check_table(table: &Table, options: &CheckOptions, today: &str) -> Result<Value, String> {
    let mut findings: Vec<Value> = Vec::new();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut add = |kind: &'static str, row: &Row, detail: Value| {
        *counts.entry(kind).or_insert(0) += 1;
        let mut entry = json!({"kind": kind, "row": row.number});
        if let (Some(target), Some(extra)) = (entry.as_object_mut(), detail.as_object()) {
            target.extend(extra.clone());
        }
        findings.push(entry);
    };

    // Repeated rows.
    if !options.duplicates_by.is_empty() {
        let columns = indexes(table, &options.duplicates_by, "")?;
        let mut first: HashMap<String, &Row> = HashMap::new();
        for row in &table.rows {
            let key = key_of(table, row, &columns);
            if columns
                .iter()
                .all(|column| table.cell(row, *column).is_empty())
            {
                continue;
            }
            match first.get(&key) {
                Some(earlier) => add(
                    "duplicate",
                    row,
                    json!({"key": key_text(table, row, &columns), "first_row": earlier.number}),
                ),
                None => {
                    first.insert(key, row);
                }
            }
        }
    }

    // Sums.
    for rule in &options.rules {
        let (target, expression) = parse_rule(rule)?;
        let target_column = table
            .column(&target)
            .ok_or_else(|| format!("{rule:?}: the table has no column {target:?}"))?;
        let mut used = Vec::new();
        columns_of(&expression, &mut used);
        let mut used_columns = HashMap::new();
        for name in used {
            let column = table
                .column(&name)
                .ok_or_else(|| format!("{rule:?}: the table has no column {name:?}"))?;
            used_columns.insert(name.to_lowercase(), column);
        }
        for row in &table.rows {
            let value_of = |name: &str| {
                used_columns
                    .get(&name.to_lowercase())
                    .and_then(|column| parse_number(table.cell(row, *column)))
            };
            let stated = parse_number(table.cell(row, target_column));
            // A program that writes a workbook leaves a formula with no saved result; its text shows
            // in place of a number. Say that, instead of "not a number".
            if table.cell(row, target_column).starts_with('=') {
                add(
                    "uncalculated",
                    row,
                    json!({
                        "rule": rule, "formula": table.cell(row, target_column),
                        "note": "the cell holds a formula with no saved result; open the file in Excel and save it so the value is stored, then check again",
                    }),
                );
                continue;
            }
            match (stated, evaluate(&expression, &value_of)) {
                (Some(stated), Some(expected)) => {
                    if (stated - expected).abs() > options.tolerance {
                        add(
                            "arithmetic",
                            row,
                            json!({
                                "rule": rule, "stated": round6(stated), "expected": round6(expected),
                                "difference": round6(stated - expected),
                            }),
                        );
                    }
                }
                // A cell that is not a number where the sum needs one cannot be checked, and is said so.
                _ if table.cell(row, target_column).is_empty() => {}
                _ => add(
                    "not_a_number",
                    row,
                    json!({"rule": rule, "note": "a cell this sum needs is empty or not a number"}),
                ),
            }
        }
    }

    // Dates.
    let bound = |text: &Option<String>| text.as_deref().and_then(parse_date);
    let (not_before, not_after) = (
        bound(&options.not_before),
        bound(&options.not_after).or_else(|| parse_date(today)),
    );
    for name in &options.date_columns {
        let column = table
            .column(name)
            .ok_or_else(|| format!("the table has no column {name:?}"))?;
        for row in &table.rows {
            let cell = table.cell(row, column);
            if cell.is_empty() {
                continue;
            }
            match parse_date(cell) {
                None => add(
                    "bad_date",
                    row,
                    json!({"column": name, "value": cell, "note": "not a date"}),
                ),
                Some(date) => {
                    if not_after.is_some_and(|latest| date > latest) {
                        add(
                            "date_late",
                            row,
                            json!({"column": name, "value": cell, "date": date_text(date), "later_than": not_after.map(date_text)}),
                        );
                    } else if not_before.is_some_and(|earliest| date < earliest) {
                        add(
                            "date_early",
                            row,
                            json!({"column": name, "value": cell, "date": date_text(date), "earlier_than": not_before.map(date_text)}),
                        );
                    }
                }
            }
        }
    }

    // Required cells.
    for name in &options.required {
        let column = table
            .column(name)
            .ok_or_else(|| format!("the table has no column {name:?}"))?;
        for row in &table.rows {
            if table.cell(row, column).is_empty() {
                add("missing", row, json!({"column": name}));
            }
        }
    }

    findings.sort_by_key(|finding| finding["row"].as_u64().unwrap_or(0));
    let total = findings.len();
    Ok(json!({
        "ok": true, "passed": total == 0, "table": table.name, "rows": table.rows.len(),
        "counts": counts, "findings_total": total, "findings": listed(findings),
        "dates_checked_up_to": not_after.map(date_text),
    }))
}

// -- tools ------------------------------------------------------------------------------------

fn table_arguments() -> Value {
    json!({
        "path": {"type": "string", "description": "A .csv, .tsv or .xlsx file, relative to the workspace."},
        "sheet": {"type": "string", "description": "The worksheet, by name (the first by default)."},
        "header_row": {"type": "integer", "description": "The row number that holds the column headings (the first row with content by default)."}
    })
}

fn strings(arguments: &Value, name: &str) -> Vec<String> {
    match arguments.get(name) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        Some(Value::String(one)) if !one.trim().is_empty() => vec![one.clone()],
        _ => Vec::new(),
    }
}

fn read_table(
    job: &crate::capability::CapabilityJob,
    spec: &Value,
) -> Result<Table, (String, &'static str)> {
    let Some(path) = spec.get("path").and_then(Value::as_str) else {
        return Err(("path is required".to_owned(), "arguments"));
    };
    let resolved = crate::capability::resolve_read_path(job, path)
        .map_err(|(code, message)| (message, code))?;
    load_table(
        &resolved,
        spec.get("sheet").and_then(Value::as_str),
        spec.get("header_row")
            .and_then(Value::as_u64)
            .map(|row| row as usize),
    )
    .map_err(|message| (message, "read_failed"))
}

pub fn reconcile_registration() -> CapabilityRegistration {
    CapabilityRegistration {
        capability_id: "table.reconcile".to_owned(),
        tool_name: "table_reconcile".to_owned(),
        side_effect: crate::skills::SideEffectClass::Read,
        description: "Line two tables (CSV, TSV or XLSX) up on a key and report exactly what \
            differs: rows only in the left, rows only in the right, repeated keys, and the cells \
            that differ in rows both have (numbers within `tolerance`). The counts are computed, \
            not estimated: report them as the tool gives them and never recount or add up \
            figures yourself. Nothing is changed. A key is one or more column headings; numbers \
            match by value (001 and 1 are the same key), text ignores case and spacing."
            .to_owned(),
        parameters: json!({
            "type": "object", "required": ["left", "right", "keys"],
            "properties": {
                "left": {"type": "object", "required": ["path"], "properties": table_arguments(), "description": "The first table."},
                "right": {"type": "object", "required": ["path"], "properties": table_arguments(), "description": "The second table."},
                "keys": {"type": "array", "items": {"type": "string"}, "description": "Column headings that identify a row; both tables need them."},
                "compare": {"type": "array", "items": {"type": "string"}, "description": "Columns to compare. Default: every column both tables have, except the keys."},
                "tolerance": {"type": "number", "description": "How far two numbers may differ and still match (default 0)."}
            }
        }),
        metadata: json!({
            "risk_level": "low", "requires_approval": false,
            "category": "read", "capabilities": ["table.reconcile"]
        }),
        grants: CapabilityGrants::read_only(),
        workspace_write: false,
        runner: Arc::new(NativeCapabilityRunner::new(|job, control| {
            let (Some(left), Some(right)) = (job.arguments.get("left"), job.arguments.get("right"))
            else {
                return CapabilityResult::failed(
                    &job.job_id,
                    "left and right are required",
                    Some("arguments"),
                );
            };
            let options = ReconcileOptions {
                keys: strings(&job.arguments, "keys"),
                compare: job
                    .arguments
                    .get("compare")
                    .map(|_| strings(&job.arguments, "compare")),
                tolerance: job
                    .arguments
                    .get("tolerance")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0)
                    .max(0.0),
            };
            if control.is_cancelled() {
                return CapabilityResult::cancelled(&job.job_id);
            }
            let tables = read_table(job, left).and_then(|l| read_table(job, right).map(|r| (l, r)));
            match tables {
                Ok((left, right)) => match reconcile(&left, &right, &options) {
                    Ok(report) => CapabilityResult::completed(&job.job_id, report),
                    Err(error) => CapabilityResult::failed(&job.job_id, &error, Some("arguments")),
                },
                Err((message, code)) => CapabilityResult::failed(&job.job_id, &message, Some(code)),
            }
        })),
    }
}

pub fn check_registration() -> CapabilityRegistration {
    CapabilityRegistration {
        capability_id: "table.check".to_owned(),
        tool_name: "table_check".to_owned(),
        side_effect: crate::skills::SideEffectClass::Read,
        description: "Check one table (CSV, TSV or XLSX), such as an invoice or expense ledger, \
            for: rows repeated on `duplicates_by` (for example the invoice number and the \
            vendor), sums that do not hold on a row (`rules`, such as amount = quantity * \
            unit_price + tax; use `backticks` around a heading with spaces), dates that are not \
            dates or fall outside `not_before`..`not_after` (later than today by default), and \
            `required` cells left empty. Findings carry the row number of the file. The counts \
            are computed: report them as the tool gives them and never add up figures yourself. \
            Nothing is changed."
            .to_owned(),
        parameters: json!({
            "type": "object", "required": ["path"],
            "properties": {
                "path": table_arguments()["path"].clone(),
                "sheet": table_arguments()["sheet"].clone(),
                "header_row": table_arguments()["header_row"].clone(),
                "duplicates_by": {"type": "array", "items": {"type": "string"}, "description": "Columns that together identify a row that may appear once."},
                "rules": {"type": "array", "items": {"type": "string"}, "description": "Sums that must hold on every row, as column = expression."},
                "date_columns": {"type": "array", "items": {"type": "string"}, "description": "Columns that hold dates."},
                "not_before": {"type": "string", "description": "Earliest allowed date, YYYY-MM-DD."},
                "not_after": {"type": "string", "description": "Latest allowed date, YYYY-MM-DD (default today)."},
                "required": {"type": "array", "items": {"type": "string"}, "description": "Columns that may not be empty."},
                "tolerance": {"type": "number", "description": "How far a sum may be out and still hold (default 0.005, half a cent)."}
            }
        }),
        metadata: json!({
            "risk_level": "low", "requires_approval": false,
            "category": "read", "capabilities": ["table.check"]
        }),
        grants: CapabilityGrants::read_only(),
        workspace_write: false,
        runner: Arc::new(NativeCapabilityRunner::new(|job, control| {
            let options = CheckOptions {
                duplicates_by: strings(&job.arguments, "duplicates_by"),
                rules: strings(&job.arguments, "rules"),
                date_columns: strings(&job.arguments, "date_columns"),
                not_before: job
                    .arguments
                    .get("not_before")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                not_after: job
                    .arguments
                    .get("not_after")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                required: strings(&job.arguments, "required"),
                tolerance: job
                    .arguments
                    .get("tolerance")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.005)
                    .max(0.0),
            };
            if control.is_cancelled() {
                return CapabilityResult::cancelled(&job.job_id);
            }
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            match read_table(job, &job.arguments).and_then(|table| {
                check_table(&table, &options, &today).map_err(|message| (message, "arguments"))
            }) {
                Ok(report) => CapabilityResult::completed(&job.job_id, report),
                Err((message, code)) => CapabilityResult::failed(&job.job_id, &message, Some(code)),
            }
        })),
    }
}

pub fn registrations() -> Vec<CapabilityRegistration> {
    vec![reconcile_registration(), check_registration()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(name: &str, text: &str) -> Table {
        table_from_text(name, text.as_bytes(), false, None).unwrap()
    }

    fn ids(value: &Value) -> Vec<String> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["key"].as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn a_table_is_read_from_csv_with_its_row_numbers() {
        let t = table(
            "a.csv",
            "\u{feff}id,name,amount\n1,Ada,10.5\n\n2,\"Grace, H\",20\n",
        );
        assert_eq!(t.header, ["id", "name", "amount"]);
        assert_eq!(t.rows.len(), 2);
        // The blank line is skipped but still counted, so the numbers are the file's own.
        assert_eq!(t.rows[1].number, 4);
        assert_eq!(t.rows[1].cells[1], "Grace, H");
        let semicolons = table("b.csv", "id;name\n1;Ada\n");
        assert_eq!(semicolons.header, ["id", "name"]);
        let shifted =
            table_from_text("c.csv", b"Report title\n\nid,name\n7,x\n", false, Some(3)).unwrap();
        assert_eq!(
            (shifted.header[0].as_str(), shifted.rows[0].number),
            ("id", 4)
        );
        assert!(table_from_text("d.csv", b"\n\n", false, None).is_err());
    }

    #[test]
    fn numbers_are_read_the_way_ledgers_write_them() {
        for (text, value) in [
            ("1,234.50", 1234.5),
            ("¥ 99", 99.0),
            ("(12.50)", -12.5),
            ("15%", 0.15),
            ("-3", -3.0),
            (" 7 ", 7.0),
            ("$1,000", 1000.0),
        ] {
            assert_eq!(parse_number(text), Some(value), "{text}");
        }
        for text in ["", "abc", "1.2.3", "NaN", "inf"] {
            assert_eq!(parse_number(text), None, "{text}");
        }
    }

    #[test]
    fn two_tables_are_lined_up_on_their_key() {
        let left = table(
            "left.csv",
            "id,name,amount\n001,Ada,10.00\n2,Grace,20\n3,Linus,30\n3,Linus,31\n",
        );
        let right = table(
            "right.csv",
            "ID,Name,Amount\n1,ada,10\n2,Grace,20.40\n4,Ken,40\n",
        );
        let report = reconcile(
            &left,
            &right,
            &ReconcileOptions {
                keys: vec!["id".into()],
                compare: None,
                tolerance: 0.0,
            },
        )
        .unwrap();
        // 001 and 1 are one key; "ada" and "Ada" differ in the name column only by case, which text
        // comparison does not set aside, so it is reported.
        assert_eq!(report["counts"]["matched"], 2);
        assert_eq!(ids(&report["only_in_left"]), ["3", "3"]);
        assert_eq!(ids(&report["only_in_right"]), ["4"]);
        let differences = report["differences"].as_array().unwrap();
        assert_eq!(differences.len(), 2, "{differences:?}");
        let amount = differences
            .iter()
            .find(|d| d["column"] == "amount")
            .unwrap();
        assert_eq!(
            (amount["left"].as_str(), amount["right"].as_str()),
            (Some("20"), Some("20.40"))
        );
        assert_eq!(amount["difference"], -0.4);
        assert_eq!(amount["left_row"], 3);
        // The key that repeats on the left is listed with both rows.
        assert_eq!(report["repeated_keys_left"][0]["rows"], json!([4, 5]));
        assert_eq!(report["reconciled"], false);

        // Within a tolerance the amounts agree.
        let loose = reconcile(
            &left,
            &right,
            &ReconcileOptions {
                keys: vec!["id".into()],
                compare: Some(vec!["amount".into()]),
                tolerance: 0.5,
            },
        )
        .unwrap();
        assert_eq!(loose["counts"]["differences"], 0);
    }

    #[test]
    fn identical_tables_reconcile_and_a_missing_column_is_named() {
        let a = table("a.csv", "id,v\n1,x\n2,y\n");
        let b = table("b.csv", "id,v\n2,y\n1,x\n");
        let options = ReconcileOptions {
            keys: vec!["id".into()],
            ..ReconcileOptions::default()
        };
        assert_eq!(reconcile(&a, &b, &options).unwrap()["reconciled"], true);
        let missing = ReconcileOptions {
            keys: vec!["code".into()],
            ..ReconcileOptions::default()
        };
        let error = reconcile(&a, &b, &missing).unwrap_err();
        assert!(
            error.contains("the left table has no column \"code\""),
            "{error}"
        );
        assert!(reconcile(&a, &b, &ReconcileOptions::default()).is_err());
    }

    #[test]
    fn a_composite_key_matches_on_every_part() {
        let a = table("a.csv", "inv,vendor,total\nA1,Acme,5\nA1,Beta,6\n");
        let b = table("b.csv", "inv,vendor,total\nA1,Beta,6\nA1,Acme,9\n");
        let options = ReconcileOptions {
            keys: vec!["inv".into(), "vendor".into()],
            ..ReconcileOptions::default()
        };
        let report = reconcile(&a, &b, &options).unwrap();
        assert_eq!(report["counts"]["matched"], 2);
        assert_eq!(report["counts"]["differences"], 1);
        assert_eq!(report["differences"][0]["key"], "A1 / Acme");
    }

    #[test]
    fn dates_are_read_in_the_forms_people_write_them() {
        for (text, date) in [
            ("2026-10-28", (2026, 10, 28)),
            ("2026/1/5", (2026, 1, 5)),
            ("2026.10.28", (2026, 10, 28)),
            ("2026年10月28日", (2026, 10, 28)),
            ("20261028", (2026, 10, 28)),
            ("46323", (2026, 10, 28)),
        ] {
            assert_eq!(parse_date(text), Some(date), "{text}");
        }
        for text in [
            "",
            "yesterday",
            "2026-13-01",
            "2026-02-30",
            "12",
            "99999999",
        ] {
            assert_eq!(parse_date(text), None, "{text}");
        }
    }

    #[test]
    fn a_sum_is_parsed_with_the_usual_precedence() {
        let (target, expression) = parse_rule("amount = quantity * `unit price` + tax").unwrap();
        assert_eq!(target, "amount");
        let values: HashMap<&str, f64> =
            [("quantity", 3.0), ("unit price", 2.0), ("tax", 1.0)].into();
        let value_of = |name: &str| values.get(name).copied();
        assert_eq!(evaluate(&expression, &value_of), Some(7.0));
        let (_, bracketed) = parse_rule("t = (a + b) / 2").unwrap();
        let ab: HashMap<&str, f64> = [("a", 3.0), ("b", 5.0)].into();
        assert_eq!(
            evaluate(&bracketed, &|name| ab.get(name).copied()),
            Some(4.0)
        );
        // Dividing by nothing is not a number, and is not a sum that holds.
        let (_, divide) = parse_rule("t = a / b").unwrap();
        let zero: HashMap<&str, f64> = [("a", 1.0), ("b", 0.0)].into();
        assert_eq!(evaluate(&divide, &|name| zero.get(name).copied()), None);
        for bad in ["amount", "= a", "t = a +", "t = (a", "t = a b", "t = a $ b"] {
            assert!(parse_rule(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_ledger_is_checked_for_repeats_sums_dates_and_gaps() {
        let ledger = table(
            "ledger.csv",
            "invoice,vendor,date,quantity,unit_price,amount\n\
             A-1,Acme,2026-01-05,2,10.00,20.00\n\
             A-2,Acme,2026-01-06,3,5.00,16.00\n\
             A-1,Acme,2026-01-07,1,1.00,1.00\n\
             A-3,Beta,2099-01-01,1,2.00,2.00\n\
             A-4,Beta,not a date,1,2.00,2.00\n\
             ,Beta,2026-01-09,1,2.00,2.00\n\
             A-5,Beta,1999-01-01,x,2.00,2.00\n",
        );
        let options = CheckOptions {
            duplicates_by: vec!["invoice".into(), "vendor".into()],
            rules: vec!["amount = quantity * unit_price".into()],
            date_columns: vec!["date".into()],
            not_before: Some("2020-01-01".into()),
            required: vec!["invoice".into()],
            tolerance: 0.005,
            ..CheckOptions::default()
        };
        let report = check_table(&ledger, &options, "2026-10-28").unwrap();
        let found: Vec<(String, u64)> = report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                (
                    f["kind"].as_str().unwrap().to_owned(),
                    f["row"].as_u64().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            found,
            [
                ("arithmetic".to_owned(), 3),
                ("duplicate".to_owned(), 4),
                ("date_late".to_owned(), 5),
                ("bad_date".to_owned(), 6),
                ("missing".to_owned(), 7),
                ("not_a_number".to_owned(), 8),
                ("date_early".to_owned(), 8),
            ]
        );
        assert_eq!(report["passed"], false);
        assert_eq!(report["counts"]["arithmetic"], 1);
        let arithmetic = &report["findings"][0];
        assert_eq!(
            (
                arithmetic["stated"].as_f64(),
                arithmetic["expected"].as_f64()
            ),
            (Some(16.0), Some(15.0))
        );
        assert_eq!(report["findings"][1]["first_row"], 2);
        assert_eq!(report["dates_checked_up_to"], "2026-10-28");

        // A clean ledger passes.
        let clean = table(
            "c.csv",
            "invoice,date,qty,price,amount\nA,2026-01-01,2,1.5,3\n",
        );
        let options = CheckOptions {
            rules: vec!["amount = qty * price".into()],
            date_columns: vec!["date".into()],
            ..CheckOptions::default()
        };
        assert_eq!(
            check_table(&clean, &options, "2026-10-28").unwrap()["passed"],
            true
        );
    }

    #[test]
    fn a_rule_or_column_that_cannot_apply_is_an_error_not_a_pass() {
        let t = table("t.csv", "a,b\n1,2\n");
        for options in [
            CheckOptions {
                rules: vec!["a = nothing + 1".into()],
                ..CheckOptions::default()
            },
            CheckOptions {
                rules: vec!["zzz = a".into()],
                ..CheckOptions::default()
            },
            CheckOptions {
                date_columns: vec!["when".into()],
                ..CheckOptions::default()
            },
            CheckOptions {
                required: vec!["c".into()],
                ..CheckOptions::default()
            },
            CheckOptions {
                duplicates_by: vec!["c".into()],
                ..CheckOptions::default()
            },
        ] {
            assert!(
                check_table(&t, &options, "2026-10-28").is_err(),
                "{options:?}"
            );
        }
    }

    #[test]
    fn findings_are_capped_but_counted_in_full() {
        let mut text = String::from("id,amount\n");
        for n in 0..250 {
            text.push_str(&format!("{n},x\n"));
        }
        let t = table("big.csv", &text);
        let options = CheckOptions {
            required: vec!["amount".into()],
            rules: vec!["id = id + 1".into()],
            ..CheckOptions::default()
        };
        let report = check_table(&t, &options, "2026-10-28").unwrap();
        assert_eq!(report["findings_total"], 250);
        assert_eq!(report["findings"].as_array().unwrap().len(), MAX_LISTED);
    }

    #[test]
    fn a_workbook_is_read_by_sheet_with_the_sheets_own_row_numbers() {
        let arguments = json!({"sheets": [
            {"name": "Orders", "header": true, "rows": [["id", "total"], ["1", "5"], ["2", "7"]]},
            {"name": "Other", "header": true, "rows": [["x"], ["y"]]}
        ]});
        let (_, bytes, _) = super::super::create("book.xlsx", &arguments).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.xlsx");
        std::fs::write(&path, bytes).unwrap();

        let orders = load_table(&path, Some("orders"), None).unwrap();
        assert_eq!(orders.header, ["id", "total"]);
        assert_eq!(
            orders.rows.iter().map(|row| row.number).collect::<Vec<_>>(),
            [2, 3]
        );
        assert_eq!(orders.name, "book.xlsx / Orders");
        assert_eq!(
            load_table(&path, None, None).unwrap().name,
            "book.xlsx / Orders"
        );
        let error = load_table(&path, Some("Missing"), None).unwrap_err();
        assert!(error.contains("\"Orders\", \"Other\""), "{error}");
        assert!(load_table(&dir.path().join("note.docx"), None, None).is_err());
    }

    #[test]
    fn a_title_row_above_the_headings_is_not_taken_for_the_headings() {
        let t = table(
            "report.csv",
            "October ledger,,,\n\nid,vendor,qty,amount\nA,Acme,2,10\nB,Beta,1,5\n",
        );
        assert_eq!(t.header, ["id", "vendor", "qty", "amount"]);
        assert_eq!(
            t.rows.iter().map(|row| row.number).collect::<Vec<_>>(),
            [4, 5]
        );
        // A sheet with a single column keeps its first row as the heading.
        assert_eq!(table("one.csv", "name\nx\ny\n").header, ["name"]);
        // A heading row with a gap is still the heading row.
        let gap = table("gap.csv", "Title\nid,,amount\n1,,2\n");
        assert_eq!(gap.header, ["id", "", "amount"]);
        // Asking for a row is always honoured.
        let asked = table_from_text("ask.csv", b"a,b\nc,d\n", false, Some(2)).unwrap();
        assert_eq!(asked.header, ["c", "d"]);
    }

    #[test]
    fn a_formula_with_no_saved_result_is_named_as_that() {
        let t = table("f.csv", "qty,price,amount\n2,3,=A2*B2\n1,2,2\n");
        let options = CheckOptions {
            rules: vec!["amount = qty * price".into()],
            ..CheckOptions::default()
        };
        let report = check_table(&t, &options, "2026-10-28").unwrap();
        assert_eq!(report["findings_total"], 1, "{report}");
        assert_eq!(report["findings"][0]["kind"], "uncalculated");
        assert_eq!(report["findings"][0]["formula"], "=A2*B2");
        assert!(report["findings"][0]["note"]
            .as_str()
            .unwrap()
            .contains("open the file in Excel"));
    }

    #[test]
    fn the_tools_are_read_only_and_ask_for_nothing() {
        for registration in registrations() {
            assert_eq!(registration.metadata["requires_approval"], false);
            assert!(!registration.workspace_write);
            assert!(registration.grants.network.is_empty());
        }
    }
}
