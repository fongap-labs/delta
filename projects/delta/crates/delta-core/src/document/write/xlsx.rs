//! XLSX: sheets of rows. Numbers stay numbers, `true`/`false` stay booleans, text stays text, and a
//! string starting with `=` is a formula that Excel calculates when the file opens.

use serde_json::Value;

use super::{array, esc, package, text_of, XML};

const MAX_SHEETS: usize = 30;
const MAX_ROWS: usize = 100_000;
const MAX_COLUMNS: usize = 256;
const MAX_SHEET_NAME: usize = 31;

const NS: &str = "xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" \
    xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"";

/// Column letters for a 0-based index: 0 -> A, 26 -> AA.
fn column_name(mut index: usize) -> String {
    let mut name = String::new();
    loop {
        name.insert(0, (b'A' + (index % 26) as u8) as char);
        if index < 26 {
            return name;
        }
        index = index / 26 - 1;
    }
}

/// Display width of text: a CJK character is about twice as wide as a Latin one.
fn display_width(text: &str) -> usize {
    text.chars()
        .map(|c| if (c as u32) >= 0x2E80 { 2 } else { 1 })
        .sum()
}

enum Cell {
    Empty,
    Number(String),
    Bool(bool),
    Text(String),
    Formula(String),
}

fn cell_of(value: &Value) -> Result<Cell, String> {
    Ok(match value {
        Value::Null => Cell::Empty,
        Value::Bool(flag) => Cell::Bool(*flag),
        Value::Number(number) => Cell::Number(number.to_string()),
        Value::String(text) if text.is_empty() => Cell::Empty,
        Value::String(text) => match text.strip_prefix('=') {
            Some(formula) if !formula.trim().is_empty() => Cell::Formula(formula.trim().to_owned()),
            _ => Cell::Text(text_of(value, "a cell")?),
        },
        _ => return Err("a cell must be text, a number, true/false or null".to_owned()),
    })
}

/// Excel's rules: at most 31 characters, none of `[]:*?/\`, not empty, unique ignoring case.
fn sheet_name(raw: &str, taken: &[String]) -> String {
    let mut name: String = raw
        .chars()
        .map(|c| if "[]:*?/\\".contains(c) { '-' } else { c })
        .collect::<String>()
        .trim()
        .trim_matches('\'')
        .chars()
        .take(MAX_SHEET_NAME)
        .collect();
    if name.is_empty() {
        name = "Sheet".to_owned();
    }
    let base = name.clone();
    let mut n = 2;
    while taken.iter().any(|t| t.eq_ignore_ascii_case(&name)) {
        let suffix = format!(" {n}");
        name = format!(
            "{}{suffix}",
            base.chars()
                .take(MAX_SHEET_NAME - suffix.len())
                .collect::<String>()
        );
        n += 1;
    }
    name
}

struct Sheet {
    name: String,
    rows: Vec<Vec<Cell>>,
    header: bool,
}

fn sheet_xml(sheet: &Sheet) -> String {
    let columns = sheet.rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut widths = vec![8usize; columns];
    for row in &sheet.rows {
        for (column, cell) in row.iter().enumerate() {
            let width = match cell {
                Cell::Text(text) => display_width(text),
                Cell::Number(text) | Cell::Formula(text) => text.len().min(14),
                Cell::Bool(_) => 6,
                Cell::Empty => 0,
            };
            widths[column] = widths[column].max(width + 2).min(60);
        }
    }
    let mut xml = format!("{XML}<worksheet {NS}>");
    xml.push_str("<sheetViews><sheetView workbookViewId=\"0\"");
    if sheet.header {
        xml.push_str("><pane ySplit=\"1\" topLeftCell=\"A2\" activePane=\"bottomLeft\" state=\"frozen\"/></sheetView>");
    } else {
        xml.push_str("/>");
    }
    xml.push_str("</sheetViews><sheetFormatPr defaultRowHeight=\"15\"/>");
    if columns > 0 {
        xml.push_str("<cols>");
        for (column, width) in widths.iter().enumerate() {
            xml.push_str(&format!(
                "<col min=\"{0}\" max=\"{0}\" width=\"{width}\" customWidth=\"1\"/>",
                column + 1
            ));
        }
        xml.push_str("</cols>");
    }
    xml.push_str("<sheetData>");
    for (row_index, row) in sheet.rows.iter().enumerate() {
        let number = row_index + 1;
        xml.push_str(&format!("<row r=\"{number}\">"));
        let style = if sheet.header && row_index == 0 {
            " s=\"1\""
        } else {
            ""
        };
        for (column, cell) in row.iter().enumerate() {
            let reference = format!("{}{number}", column_name(column));
            match cell {
                Cell::Empty => {}
                Cell::Number(value) => xml.push_str(&format!("<c r=\"{reference}\"{style}><v>{value}</v></c>")),
                Cell::Bool(flag) => xml.push_str(&format!("<c r=\"{reference}\"{style} t=\"b\"><v>{}</v></c>", u8::from(*flag))),
                Cell::Text(text) => xml.push_str(&format!(
                    "<c r=\"{reference}\"{style} t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>",
                    esc(text)
                )),
                Cell::Formula(formula) => xml.push_str(&format!("<c r=\"{reference}\"{style}><f>{}</f></c>", esc(formula))),
            }
        }
        xml.push_str("</row>");
    }
    xml.push_str("</sheetData></worksheet>");
    xml
}

pub fn build(arguments: &Value) -> Result<Vec<u8>, String> {
    let raw_sheets = array(arguments, "sheets")?;
    if raw_sheets.is_empty() {
        return Err("sheets must not be empty".to_owned());
    }
    if raw_sheets.len() > MAX_SHEETS {
        return Err(format!("a workbook may have at most {MAX_SHEETS} sheets"));
    }
    let mut sheets: Vec<Sheet> = Vec::new();
    for (index, raw) in raw_sheets.iter().enumerate() {
        let at = |what: &str| format!("sheet {}: {what}", index + 1);
        let taken: Vec<String> = sheets.iter().map(|s| s.name.clone()).collect();
        let name = sheet_name(
            raw.get("name").and_then(Value::as_str).unwrap_or(""),
            &taken,
        );
        let raw_rows = array(raw, "rows").map_err(|e| at(&e))?;
        if raw_rows.len() > MAX_ROWS {
            return Err(at(&format!("at most {MAX_ROWS} rows")));
        }
        let mut rows = Vec::with_capacity(raw_rows.len());
        for row in raw_rows {
            let cells = row
                .as_array()
                .ok_or_else(|| at("each row must be a list of cells"))?;
            if cells.len() > MAX_COLUMNS {
                return Err(at(&format!("at most {MAX_COLUMNS} columns")));
            }
            rows.push(
                cells
                    .iter()
                    .map(cell_of)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| at(&e))?,
            );
        }
        let header =
            raw.get("header").and_then(Value::as_bool).unwrap_or(false) && !rows.is_empty();
        sheets.push(Sheet { name, rows, header });
    }

    let mut parts: Vec<(String, String)> = Vec::new();
    let mut content_types = format!(
        "{XML}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
         <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
         <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
         <Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>\
         <Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>"
    );
    let mut workbook = format!("{XML}<workbook {NS}><sheets>");
    let mut rels = format!(
        "{XML}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">"
    );
    for (index, sheet) in sheets.iter().enumerate() {
        let n = index + 1;
        content_types.push_str(&format!(
            "<Override PartName=\"/xl/worksheets/sheet{n}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"
        ));
        workbook.push_str(&format!(
            "<sheet name=\"{}\" sheetId=\"{n}\" r:id=\"rId{n}\"/>",
            esc(&sheet.name)
        ));
        rels.push_str(&format!(
            "<Relationship Id=\"rId{n}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{n}.xml\"/>"
        ));
        parts.push((format!("xl/worksheets/sheet{n}.xml"), sheet_xml(sheet)));
    }
    content_types.push_str("</Types>");
    // Formulas written without a stored result are calculated when the file is opened.
    workbook.push_str("</sheets><calcPr calcId=\"191029\" fullCalcOnLoad=\"1\"/></workbook>");
    rels.push_str(&format!(
        "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/></Relationships>",
        sheets.len() + 1
    ));

    let styles = format!(
        "{XML}<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">\
         <fonts count=\"2\"><font><sz val=\"11\"/><name val=\"Calibri\"/><family val=\"2\"/></font>\
         <font><b/><sz val=\"11\"/><name val=\"Calibri\"/><family val=\"2\"/></font></fonts>\
         <fills count=\"3\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill>\
         <fill><patternFill patternType=\"solid\"><fgColor rgb=\"FFD9E2F3\"/><bgColor indexed=\"64\"/></patternFill></fill></fills>\
         <borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders>\
         <cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>\
         <cellXfs count=\"2\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/>\
         <xf numFmtId=\"0\" fontId=\"1\" fillId=\"2\" borderId=\"0\" xfId=\"0\" applyFont=\"1\" applyFill=\"1\"/></cellXfs>\
         <cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles></styleSheet>"
    );
    parts.insert(0, ("[Content_Types].xml".to_owned(), content_types));
    parts.insert(
        1,
        (
            "_rels/.rels".to_owned(),
            format!(
                "{XML}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
                 <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>"
            ),
        ),
    );
    parts.insert(2, ("xl/workbook.xml".to_owned(), workbook));
    parts.insert(3, ("xl/_rels/workbook.xml.rels".to_owned(), rels));
    parts.insert(4, ("xl/styles.xml".to_owned(), styles));
    package(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_names_roll_over_like_excel() {
        assert_eq!(column_name(0), "A");
        assert_eq!(column_name(25), "Z");
        assert_eq!(column_name(26), "AA");
        assert_eq!(column_name(27), "AB");
        assert_eq!(column_name(701), "ZZ");
        assert_eq!(column_name(702), "AAA");
    }

    #[test]
    fn sheet_names_follow_excels_rules_and_stay_unique() {
        assert_eq!(sheet_name("a/b:c", &[]), "a-b-c");
        assert_eq!(sheet_name("", &[]), "Sheet");
        assert_eq!(sheet_name("x".repeat(40).as_str(), &[]).chars().count(), 31);
        assert_eq!(sheet_name("Data", &["data".to_owned()]), "Data 2");
        assert_eq!(
            sheet_name("Data", &["Data".to_owned(), "Data 2".to_owned()]),
            "Data 3"
        );
    }
}
