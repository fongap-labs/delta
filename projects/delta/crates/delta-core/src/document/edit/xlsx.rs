//! XLSX: cells set in an existing worksheet.
//!
//! Only the cells an edit names are written. A cell that exists keeps its style and every other
//! attribute; a cell that does not exist is added in column order, in its row, in row order. Text
//! is stored as an inline string so the shared string table, which other cells point into, is not
//! touched. Text that starts with `=` becomes a formula. Merged cells, other formulas, column
//! widths and everything else in the sheet come through as they were.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer};
use serde_json::{json, Value};

use super::super::ooxml::{self, local, local_end, xml_error};
use super::super::write::strip_xml_controls;
use super::{Changes, MAX_CHARS};

/// Most cells one call may write.
const MAX_CELLS: usize = 10_000;
const MAX_ROW: usize = 1_048_576;
const MAX_COLUMN: usize = 16_384;

struct CellWrite {
    row: usize,
    column: usize,
    value: Value,
}

pub(super) fn edit(bytes: &[u8], edits: &[Value]) -> Result<Changes, String> {
    let mut archive = ooxml::open(bytes)?;
    let sheets = ooxml::sheet_parts(&mut archive)?;
    let mut by_sheet: BTreeMap<usize, Vec<CellWrite>> = BTreeMap::new();
    let mut applied = Vec::new();
    let mut total = 0usize;
    for (index, edit) in edits.iter().enumerate() {
        let number = index + 1;
        let name = edit
            .get("sheet")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("edit {number}: sheet is required"))?;
        let sheet = sheets
            .iter()
            .position(|(sheet_name, _)| sheet_name == name)
            .ok_or_else(|| format!("edit {number}: there is no sheet named '{name}'"))?;
        let cells = cells_of(edit, number)?;
        total += cells.len();
        if total > MAX_CELLS {
            return Err(format!(
                "at most {MAX_CELLS} cells may be written in one call"
            ));
        }
        applied.push(json!({"edit": number, "sheet": name, "cells": cells.len()}));
        by_sheet.entry(sheet).or_default().extend(cells);
    }
    let mut parts = Vec::new();
    let mut rows = Vec::new();
    for (sheet, writes) in &by_sheet {
        let (name, part) = &sheets[*sheet];
        let xml = ooxml::required_part(&mut archive, part)?;
        parts.push((part.clone(), apply(&xml, part, writes)?));
        rows.extend(writes.iter().map(|write| (name.clone(), write.row)));
    }
    rows.sort();
    rows.dedup();
    Ok(Changes {
        parts,
        applied,
        paragraphs: Vec::new(),
        rows,
    })
}

/// The cells one edit writes: (row, column, value), with the value checked.
fn cells_of(edit: &Value, number: usize) -> Result<Vec<CellWrite>, String> {
    match (edit.get("cell"), edit.get("range")) {
        (Some(cell), None) => {
            let (row, column) = reference(
                cell.as_str()
                    .ok_or_else(|| format!("edit {number}: cell must be text such as \"B2\""))?,
                number,
            )?;
            let value = edit.get("value").ok_or_else(|| {
                format!("edit {number}: value is required (null clears the cell)")
            })?;
            Ok(vec![CellWrite {
                row,
                column,
                value: checked(value, number)?,
            }])
        }
        (None, Some(range)) => {
            let range = range
                .as_str()
                .ok_or_else(|| format!("edit {number}: range must be text such as \"A1:C3\""))?;
            let (from, to) = range
                .split_once(':')
                .ok_or_else(|| format!("edit {number}: range must look like \"A1:C3\""))?;
            let (row_a, column_a) = reference(from, number)?;
            let (row_b, column_b) = reference(to, number)?;
            let (top, bottom) = (row_a.min(row_b), row_a.max(row_b));
            let (left, right) = (column_a.min(column_b), column_a.max(column_b));
            let values = edit
                .get("values")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    format!("edit {number}: values is required with a range (a list of rows)")
                })?;
            if values.len() != bottom - top + 1 {
                return Err(format!(
                    "edit {number}: the range has {} rows but values has {}",
                    bottom - top + 1,
                    values.len()
                ));
            }
            let mut cells = Vec::new();
            for (offset, row_values) in values.iter().enumerate() {
                let row_values = row_values
                    .as_array()
                    .ok_or_else(|| format!("edit {number}: each row of values must be a list"))?;
                if row_values.len() != right - left + 1 {
                    return Err(format!(
                        "edit {number}: the range has {} columns but row {} of values has {}",
                        right - left + 1,
                        offset + 1,
                        row_values.len()
                    ));
                }
                for (position, value) in row_values.iter().enumerate() {
                    cells.push(CellWrite {
                        row: top + offset,
                        column: left + position,
                        value: checked(value, number)?,
                    });
                }
            }
            Ok(cells)
        }
        _ => Err(format!(
            "edit {number}: give either cell and value, or range and values"
        )),
    }
}

/// A cell reference such as `B2` as (row, column), both 1-based.
fn reference(text: &str, number: usize) -> Result<(usize, usize), String> {
    let text = text.trim();
    let letters = text.chars().take_while(char::is_ascii_alphabetic).count();
    let (column_text, row_text) = text.split_at(letters);
    let column = if column_text.is_empty() || column_text.len() > 3 {
        0
    } else {
        ooxml::column_of(column_text) + 1
    };
    let row = row_text.parse::<usize>().unwrap_or(0);
    if column == 0 || column > MAX_COLUMN || row == 0 || row > MAX_ROW {
        return Err(format!(
            "edit {number}: '{text}' is not a cell reference such as \"B2\""
        ));
    }
    Ok((row, column))
}

fn checked(value: &Value, number: usize) -> Result<Value, String> {
    match value {
        Value::String(text) => {
            if text.chars().count() > MAX_CHARS {
                return Err(format!(
                    "edit {number}: a cell value is too long (over {MAX_CHARS} characters)"
                ));
            }
            Ok(Value::String(strip_xml_controls(text)))
        }
        Value::Number(_) | Value::Bool(_) | Value::Null => Ok(value.clone()),
        _ => Err(format!(
            "edit {number}: a cell value must be text, a number, true/false or null"
        )),
    }
}

// -- rewriting the sheet -----------------------------------------------------------------------

enum Slot {
    Cell {
        column: usize,
        events: Vec<Event<'static>>,
    },
    Other(Event<'static>),
}

struct Row {
    number: usize,
    start: BytesStart<'static>,
    slots: Vec<Slot>,
    modified: bool,
}

enum Entry {
    Row(Row),
    Other(Event<'static>),
}

fn events_of(xml: &[u8], part: &str) -> Result<Vec<Event<'static>>, String> {
    let mut reader = Reader::from_reader(xml);
    let mut events = Vec::new();
    loop {
        let event = reader
            .read_event()
            .map_err(|error| xml_error(part, error))?
            .into_owned();
        if matches!(event, Event::Eof) {
            return Ok(events);
        }
        events.push(event);
    }
}

fn attribute_of(start: &BytesStart<'_>, name: &str) -> Option<String> {
    start.attributes().flatten().find_map(|attribute| {
        (attribute.key.as_ref() == name.as_bytes())
            .then(|| {
                attribute
                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                    .ok()
                    .map(|value| value.into_owned())
            })
            .flatten()
    })
}

fn apply(xml: &[u8], part: &str, writes: &[CellWrite]) -> Result<Vec<u8>, String> {
    let events = events_of(xml, part)?;
    let at = events
        .iter()
        .position(|event| match event {
            Event::Start(start) | Event::Empty(start) => local(start) == "sheetData",
            _ => false,
        })
        .ok_or_else(|| format!("{part} has no sheetData; it is not a complete worksheet"))?;
    let (sheet_start, is_empty_element) = match &events[at] {
        Event::Start(start) => (start.clone(), false),
        Event::Empty(start) => (start.clone(), true),
        _ => return Err(format!("{part} could not be read as a worksheet")),
    };
    let end = if is_empty_element {
        at
    } else {
        events[at + 1..]
            .iter()
            .position(|event| matches!(event, Event::End(end) if local_end(end) == "sheetData"))
            .map(|position| at + 1 + position)
            .ok_or_else(|| format!("{part} has a sheetData that is never closed"))?
    };
    let name = String::from_utf8_lossy(sheet_start.name().as_ref()).into_owned();
    let prefix = name.strip_suffix("sheetData").unwrap_or("").to_owned();

    let mut entries = if is_empty_element {
        Vec::new()
    } else {
        read_entries(&events[at + 1..end])?
    };
    for write in writes {
        put(&mut entries, &prefix, write);
    }

    let mut out: Vec<Event<'static>> = events[..at].to_vec();
    out.push(Event::Start(sheet_start));
    out.extend(flatten(entries));
    out.push(Event::End(BytesEnd::new(name)));
    out.extend(events[end + 1..].iter().cloned());
    let mut writer = Writer::new(Vec::new());
    for event in out {
        writer
            .write_event(event)
            .map_err(|error| format!("could not write {part}: {error}"))?;
    }
    Ok(writer.into_inner())
}

/// The children of `sheetData` as rows, each with its cells; anything else is kept in place.
fn read_entries(events: &[Event<'static>]) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    let mut index = 0;
    let mut previous_row = 0usize;
    while index < events.len() {
        match &events[index] {
            Event::Start(start) if local(start) == "row" => {
                let number = attribute_of(start, "r")
                    .and_then(|r| r.parse().ok())
                    .unwrap_or(previous_row + 1);
                previous_row = number;
                let close = events[index + 1..]
                    .iter()
                    .position(|event| matches!(event, Event::End(end) if local_end(end) == "row"))
                    .map(|position| index + 1 + position)
                    .ok_or_else(|| "a worksheet row is never closed".to_owned())?;
                entries.push(Entry::Row(Row {
                    number,
                    start: start.clone(),
                    slots: read_slots(&events[index + 1..close])?,
                    modified: false,
                }));
                index = close + 1;
            }
            Event::Empty(start) if local(start) == "row" => {
                let number = attribute_of(start, "r")
                    .and_then(|r| r.parse().ok())
                    .unwrap_or(previous_row + 1);
                previous_row = number;
                entries.push(Entry::Row(Row {
                    number,
                    start: start.clone(),
                    slots: Vec::new(),
                    modified: false,
                }));
                index += 1;
            }
            other => {
                entries.push(Entry::Other(other.clone()));
                index += 1;
            }
        }
    }
    Ok(entries)
}

fn read_slots(events: &[Event<'static>]) -> Result<Vec<Slot>, String> {
    let mut slots = Vec::new();
    let mut index = 0;
    let mut cells = 0usize;
    while index < events.len() {
        let (start, last) = match &events[index] {
            Event::Start(start) if local(start) == "c" => {
                let close = events[index + 1..]
                    .iter()
                    .position(|event| matches!(event, Event::End(end) if local_end(end) == "c"))
                    .map(|position| index + 1 + position)
                    .ok_or_else(|| "a worksheet cell is never closed".to_owned())?;
                (start, close)
            }
            Event::Empty(start) if local(start) == "c" => (start, index),
            other => {
                slots.push(Slot::Other(other.clone()));
                index += 1;
                continue;
            }
        };
        let column = attribute_of(start, "r")
            .map(|r| ooxml::column_of(&r))
            .unwrap_or(cells);
        cells += 1;
        slots.push(Slot::Cell {
            column: column + 1,
            events: events[index..=last].to_vec(),
        });
        index = last + 1;
    }
    Ok(slots)
}

fn column_letters(mut column: usize) -> String {
    let mut letters = Vec::new();
    while column > 0 {
        column -= 1;
        letters.push(b'A' + (column % 26) as u8);
        column /= 26;
    }
    letters.reverse();
    String::from_utf8_lossy(&letters).into_owned()
}

/// Where a new item goes among siblings ordered by `key`: after the last one before it, else
/// before the first one after it, else at the end.
fn insert_position(keys: &[Option<usize>], key: usize) -> usize {
    let after = keys
        .iter()
        .rposition(|existing| matches!(existing, Some(existing) if *existing < key));
    let before = keys
        .iter()
        .position(|existing| matches!(existing, Some(existing) if *existing > key));
    match (after, before) {
        (Some(after), _) => after + 1,
        (None, Some(before)) => before,
        (None, None) => keys.len(),
    }
}

fn put(entries: &mut Vec<Entry>, prefix: &str, write: &CellWrite) {
    let existing = entries
        .iter()
        .position(|entry| matches!(entry, Entry::Row(row) if row.number == write.row));
    let position = match existing {
        Some(position) => position,
        None => {
            let keys: Vec<Option<usize>> = entries
                .iter()
                .map(|entry| match entry {
                    Entry::Row(row) => Some(row.number),
                    Entry::Other(_) => None,
                })
                .collect();
            let at = insert_position(&keys, write.row);
            let mut start = BytesStart::new(format!("{prefix}row"));
            start.push_attribute(("r", write.row.to_string().as_str()));
            entries.insert(
                at,
                Entry::Row(Row {
                    number: write.row,
                    start,
                    slots: Vec::new(),
                    modified: false,
                }),
            );
            at
        }
    };
    let Entry::Row(row) = &mut entries[position] else {
        return;
    };
    row.modified = true;
    let reference = format!("{}{}", column_letters(write.column), write.row);
    let found = row
        .slots
        .iter()
        .position(|slot| matches!(slot, Slot::Cell { column, .. } if *column == write.column));
    match found {
        Some(found) => {
            let old = match &row.slots[found] {
                Slot::Cell { events, .. } => match events.first() {
                    Some(Event::Start(start)) | Some(Event::Empty(start)) => Some(start.clone()),
                    _ => None,
                },
                Slot::Other(_) => None,
            };
            row.slots[found] = Slot::Cell {
                column: write.column,
                events: cell_events(prefix, &reference, old.as_ref(), &write.value),
            };
        }
        None => {
            let keys: Vec<Option<usize>> = row
                .slots
                .iter()
                .map(|slot| match slot {
                    Slot::Cell { column, .. } => Some(*column),
                    Slot::Other(_) => None,
                })
                .collect();
            let at = insert_position(&keys, write.column);
            row.slots.insert(
                at,
                Slot::Cell {
                    column: write.column,
                    events: cell_events(prefix, &reference, None, &write.value),
                },
            );
        }
    }
}

fn element(prefix: &str, name: &str, text: &str) -> Vec<Event<'static>> {
    let tag = format!("{prefix}{name}");
    let mut events = vec![Event::Start(BytesStart::new(tag.clone()))];
    if !text.is_empty() {
        events.push(Event::Text(BytesText::new(text).into_owned()));
    }
    events.push(Event::End(BytesEnd::new(tag)));
    events
}

fn cell_events(
    prefix: &str,
    reference: &str,
    old: Option<&BytesStart<'static>>,
    value: &Value,
) -> Vec<Event<'static>> {
    let (cell_type, children): (Option<&str>, Vec<Event<'static>>) = match value {
        Value::String(text) if text.len() > 1 && text.starts_with('=') => {
            (None, element(prefix, "f", &text[1..]))
        }
        Value::String(text) => {
            let tag = format!("{prefix}t");
            let mut text_start = BytesStart::new(tag.clone());
            if text.starts_with(char::is_whitespace) || text.ends_with(char::is_whitespace) {
                text_start.push_attribute(("xml:space", "preserve"));
            }
            let mut events = vec![
                Event::Start(BytesStart::new(format!("{prefix}is"))),
                Event::Start(text_start),
            ];
            if !text.is_empty() {
                events.push(Event::Text(BytesText::new(text).into_owned()));
            }
            events.push(Event::End(BytesEnd::new(tag)));
            events.push(Event::End(BytesEnd::new(format!("{prefix}is"))));
            (Some("inlineStr"), events)
        }
        Value::Number(number) => (None, element(prefix, "v", &number.to_string())),
        Value::Bool(flag) => (
            Some("b"),
            element(prefix, "v", if *flag { "1" } else { "0" }),
        ),
        _ => (None, Vec::new()),
    };
    let tag = format!("{prefix}c");
    let mut start = BytesStart::new(tag.clone());
    start.push_attribute(("r", reference));
    if let Some(old) = old {
        for attribute in old.attributes().flatten() {
            let key = String::from_utf8_lossy(attribute.key.as_ref()).into_owned();
            if key == "r" || key == "t" {
                continue;
            }
            let text = attribute
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map(|value| value.into_owned())
                .unwrap_or_default();
            start.push_attribute((key.as_str(), text.as_str()));
        }
    }
    if let Some(cell_type) = cell_type {
        start.push_attribute(("t", cell_type));
    }
    if children.is_empty() {
        return vec![Event::Empty(start)];
    }
    let mut events = vec![Event::Start(start)];
    events.extend(children);
    events.push(Event::End(BytesEnd::new(tag)));
    events
}

fn flatten(entries: Vec<Entry>) -> Vec<Event<'static>> {
    let mut out = Vec::new();
    for entry in entries {
        let row = match entry {
            Entry::Other(event) => {
                out.push(event);
                continue;
            }
            Entry::Row(row) => row,
        };
        let name = String::from_utf8_lossy(row.start.name().as_ref()).into_owned();
        let start = if row.modified {
            // `spans` is only a hint about the columns in use and a new cell can fall outside it.
            let mut start = BytesStart::new(name.clone());
            for attribute in row.start.attributes().flatten() {
                let key = String::from_utf8_lossy(attribute.key.as_ref()).into_owned();
                if key == "spans" {
                    continue;
                }
                let text = attribute
                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                    .map(|value| value.into_owned())
                    .unwrap_or_default();
                start.push_attribute((key.as_str(), text.as_str()));
            }
            start
        } else {
            row.start.clone()
        };
        if row.slots.is_empty() {
            out.push(Event::Empty(start));
            continue;
        }
        out.push(Event::Start(start));
        for slot in row.slots {
            match slot {
                Slot::Cell { events, .. } => out.extend(events),
                Slot::Other(event) => out.push(event),
            }
        }
        out.push(Event::End(BytesEnd::new(name)));
    }
    out
}

// -- checking the result -----------------------------------------------------------------------

/// The workbook after the edit must have the same sheets, and every row an edit did not write to
/// must read exactly as before.
pub(super) fn verify(
    before: &[u8],
    after: &[u8],
    touched: &[(String, usize)],
) -> Result<(), String> {
    let old = ooxml::read_xlsx(before)?;
    let new = ooxml::read_xlsx(after)?;
    if old.len() != new.len() || old.iter().zip(&new).any(|(a, b)| a.name != b.name) {
        return Err(
            "the edit changed the sheets of the workbook, so nothing was changed".to_owned(),
        );
    }
    for (left, right) in old.iter().zip(&new) {
        let rows_of = |sheet: &ooxml::Sheet| -> BTreeMap<usize, String> {
            sheet
                .lines
                .iter()
                .filter_map(|line| {
                    let (number, _) = line.split_once(": ")?;
                    Some((number.parse().ok()?, line.clone()))
                })
                .collect()
        };
        let (before_rows, after_rows) = (rows_of(left), rows_of(right));
        let written: HashSet<usize> = touched
            .iter()
            .filter(|(sheet, _)| *sheet == left.name)
            .map(|(_, row)| *row)
            .collect();
        let all: BTreeSet<usize> = before_rows
            .keys()
            .chain(after_rows.keys())
            .copied()
            .collect();
        for row in all {
            if !written.contains(&row) && before_rows.get(&row) != after_rows.get(&row) {
                return Err(format!(
                    "the edit changed row {row} of '{}', which it was not asked to, so nothing was changed",
                    left.name
                ));
            }
        }
    }
    Ok(())
}
