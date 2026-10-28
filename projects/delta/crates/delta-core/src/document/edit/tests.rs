use std::io::Write;

use serde_json::json;

use super::*;

/// A ZIP of `(part name, content)` pairs, the container every OOXML file is.
fn zip_of(parts: &[(&str, &str)]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut out));
        for (name, content) in parts {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(content.as_bytes()).unwrap();
        }
        writer.finish().unwrap();
    }
    out
}

fn part(bytes: &[u8], name: &str) -> String {
    let mut archive = ooxml::open(bytes).unwrap();
    String::from_utf8(ooxml::required_part(&mut archive, name).unwrap()).unwrap()
}

fn run(path: &str, bytes: &[u8], edits: Value) -> Result<(Vec<u8>, Value), String> {
    edit(path, bytes, &json!({"path": path, "edits": edits}))
}

// -- DOCX ------------------------------------------------------------------------------------

const W: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"";
const STYLES: &str = "<styles><style id=\"Strong\"/></styles>";

fn docx(body: &str) -> Vec<u8> {
    zip_of(&[
        ("[Content_Types].xml", "<Types/>"),
        ("word/styles.xml", STYLES),
        (
            "word/document.xml",
            &format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:document {W}><w:body>{body}</w:body></w:document>"
            ),
        ),
    ])
}

fn edit_docx(body: &str, edits: Value) -> Result<(Vec<u8>, Value), String> {
    run("a.docx", &docx(body), edits)
}

fn body_of(bytes: &[u8]) -> String {
    part(bytes, "word/document.xml")
}

fn docx_text(bytes: &[u8]) -> String {
    parse(Kind::Docx, bytes)
        .unwrap()
        .blocks
        .iter()
        .map(|block| block.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn text_in_one_run_is_replaced_and_the_run_keeps_its_formatting() {
    let body = "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"3\"/></w:numPr></w:pPr>\
                <w:r><w:rPr><w:b/></w:rPr><w:t>Revenue grew in Q3</w:t></w:r></w:p>";
    let (edited, summary) = edit_docx(body, json!([{"find": "Q3", "replace": "Q4"}])).unwrap();
    let xml = body_of(&edited);
    assert!(
        xml.contains("<w:rPr><w:b/></w:rPr><w:t>Revenue grew in Q4</w:t>"),
        "{xml}"
    );
    assert!(xml.contains("<w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"3\"/></w:numPr>"));
    assert_eq!(summary["edits"][0]["replaced"], 1);
    assert_eq!(summary["kind"], "docx");
}

#[test]
fn a_phrase_across_two_runs_takes_the_formatting_of_the_run_it_starts_in() {
    let body = "<w:p><w:r><w:rPr><w:b/></w:rPr><w:t xml:space=\"preserve\">Hello big </w:t></w:r>\
                <w:r><w:rPr><w:i/></w:rPr><w:t>world today</w:t></w:r></w:p>";
    let (edited, _) = edit_docx(
        body,
        json!([{"find": "big world", "replace": "small planet"}]),
    )
    .unwrap();
    let xml = body_of(&edited);
    assert!(
        xml.contains("<w:rPr><w:b/></w:rPr><w:t xml:space=\"preserve\">Hello small planet</w:t>"),
        "{xml}"
    );
    assert!(
        xml.contains("<w:rPr><w:i/></w:rPr><w:t xml:space=\"preserve\"> today</w:t>"),
        "the rest of the second run keeps its own formatting: {xml}"
    );
    assert_eq!(docx_text(&edited), "Hello small planet today");
}

#[test]
fn a_phrase_across_three_runs_leaves_the_middle_run_empty() {
    let body = "<w:p><w:r><w:t xml:space=\"preserve\">Alpha </w:t></w:r><w:r><w:t xml:space=\"preserve\">Beta </w:t></w:r>\
                <w:r><w:t>Gamma</w:t></w:r></w:p>";
    let (edited, _) =
        edit_docx(body, json!([{"find": "Alpha Beta Gamma", "replace": "X"}])).unwrap();
    assert_eq!(docx_text(&edited), "X");
    assert_eq!(
        body_of(&edited).matches("<w:r>").count(),
        3,
        "no run was removed"
    );
}

#[test]
fn a_find_that_appears_twice_is_refused_unless_an_occurrence_is_given() {
    let body = "<w:p><w:r><w:t>cat and cat</w:t></w:r></w:p>";
    let error = edit_docx(body, json!([{"find": "cat", "replace": "dog"}])).unwrap_err();
    assert!(error.contains("appears 2 times"), "{error}");
    let (second, _) = edit_docx(
        body,
        json!([{"find": "cat", "replace": "dog", "occurrence": 2}]),
    )
    .unwrap();
    assert_eq!(docx_text(&second), "cat and dog");
    let (all, summary) = edit_docx(
        body,
        json!([{"find": "cat", "replace": "dog", "occurrence": "all"}]),
    )
    .unwrap();
    assert_eq!(docx_text(&all), "dog and dog");
    assert_eq!(summary["edits"][0]["replaced"], 2);
}

#[test]
fn a_missing_text_and_an_occurrence_that_does_not_exist_are_refused() {
    let body = "<w:p><w:r><w:t>one cat</w:t></w:r></w:p>";
    let missing = edit_docx(body, json!([{"find": "dog", "replace": "x"}])).unwrap_err();
    assert!(missing.contains("was not found"), "{missing}");
    let late = edit_docx(
        body,
        json!([{"find": "cat", "replace": "x", "occurrence": 3}]),
    )
    .unwrap_err();
    assert!(late.contains("occurrence 3 does not exist"), "{late}");
    for bad in [json!(0), json!(-1), json!("some"), json!(1.5)] {
        let error = edit_docx(
            body,
            json!([{"find": "cat", "replace": "x", "occurrence": bad}]),
        )
        .unwrap_err();
        assert!(error.contains("occurrence must be"), "{error}");
    }
}

#[test]
fn text_in_a_table_cell_is_edited_and_the_table_stays_a_table() {
    let body = "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Region</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Revenue</w:t></w:r></w:p></w:tc></w:tr>\
                <w:tr><w:tc><w:p><w:r><w:t>East</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>10</w:t></w:r></w:p></w:tc></w:tr></w:tbl>";
    let (edited, _) = edit_docx(body, json!([{"find": "10", "replace": "12"}])).unwrap();
    assert_eq!(docx_text(&edited), "Region | Revenue\nEast | 12");
}

#[test]
fn a_replacement_with_markup_characters_is_escaped_and_reads_back_as_typed() {
    let body = "<w:p><w:r><w:t>team</w:t></w:r></w:p>";
    let (edited, _) = edit_docx(
        body,
        json!([{"find": "team", "replace": "R&D <core> \"x\""}]),
    )
    .unwrap();
    let xml = body_of(&edited);
    assert!(xml.contains("R&amp;D &lt;core&gt;"), "{xml}");
    assert_eq!(docx_text(&edited), "R&D <core> \"x\"");
}

#[test]
fn text_stored_with_entities_is_found_by_its_plain_form() {
    let body = "<w:p><w:r><w:t>Scope &amp; goals &#x4e2d;&#25991;</w:t></w:r></w:p>";
    let (edited, _) = edit_docx(
        body,
        json!([{"find": "Scope & goals", "replace": "Aims"}, {"find": "中文", "replace": "English"}]),
    )
    .unwrap();
    assert_eq!(docx_text(&edited), "Aims English");
}

#[test]
fn chinese_text_is_replaced_by_characters_not_bytes() {
    let body = "<w:p><w:r><w:t>华东区销售额</w:t></w:r><w:r><w:t>增长</w:t></w:r></w:p>";
    let (edited, _) =
        edit_docx(body, json!([{"find": "销售额增", "replace": "收入上升"}])).unwrap();
    assert_eq!(docx_text(&edited), "华东区收入上升长");
}

#[test]
fn a_replacement_with_edge_spaces_keeps_them() {
    let body = "<w:p><w:r><w:t>a-b</w:t></w:r></w:p>";
    let (edited, _) = edit_docx(body, json!([{"find": "-", "replace": " and "}])).unwrap();
    assert!(body_of(&edited).contains("<w:t>a and b</w:t>"));
    let (padded, _) = edit_docx(body, json!([{"find": "a-b", "replace": " padded "}])).unwrap();
    assert!(body_of(&padded).contains("<w:t xml:space=\"preserve\"> padded </w:t>"));
}

#[test]
fn an_empty_replacement_deletes_the_text() {
    let body = "<w:p><w:r><w:t>keep this, drop that</w:t></w:r></w:p>";
    let (edited, _) = edit_docx(body, json!([{"find": ", drop that", "replace": ""}])).unwrap();
    assert_eq!(docx_text(&edited), "keep this");
}

#[test]
fn tracked_deletions_and_other_paragraphs_are_not_matched() {
    let body = "<w:p><w:r><w:t>kept</w:t></w:r><w:r><w:delText>old words</w:delText></w:r></w:p>\
                <w:p><w:r><w:t>old</w:t></w:r></w:p><w:p><w:r><w:t> words</w:t></w:r></w:p>";
    assert!(edit_docx(body, json!([{"find": "old words", "replace": "x"}])).is_err());
}

#[test]
fn edits_apply_in_order_and_later_ones_see_earlier_ones() {
    let body = "<w:p><w:r><w:t>one two</w:t></w:r></w:p>";
    let (edited, summary) = edit_docx(
        body,
        json!([{"find": "one", "replace": "1"}, {"find": "1 two", "replace": "1 2"}]),
    )
    .unwrap();
    assert_eq!(docx_text(&edited), "1 2");
    assert_eq!(summary["edits"].as_array().unwrap().len(), 2);
}

#[test]
fn several_matches_in_one_paragraph_keep_their_offsets() {
    let body = "<w:p><w:r><w:t>ab ab ab</w:t></w:r></w:p>";
    let (edited, _) = edit_docx(
        body,
        json!([{"find": "ab", "replace": "xyz", "occurrence": "all"}]),
    )
    .unwrap();
    assert_eq!(docx_text(&edited), "xyz xyz xyz");
}

#[test]
fn every_part_the_edit_did_not_touch_is_copied_unchanged() {
    let original = docx("<w:p><w:r><w:t>text</w:t></w:r></w:p>");
    let (edited, _) = run(
        "a.docx",
        &original,
        json!([{"find": "text", "replace": "words"}]),
    )
    .unwrap();
    assert_eq!(part(&edited, "word/styles.xml"), STYLES);
    assert_eq!(part(&edited, "[Content_Types].xml"), "<Types/>");
    let (a, b) = (
        ooxml::open(&original).unwrap(),
        ooxml::open(&edited).unwrap(),
    );
    let names = |archive: &ooxml::Archive<'_>| {
        let mut names: Vec<String> = archive.file_names().map(str::to_owned).collect();
        names.sort();
        names
    };
    assert_eq!(names(&a), names(&b));
}

#[test]
fn xml_outside_the_changed_run_comes_through_byte_for_byte() {
    let body = "<w:p w:rsidR=\"00A1\"><w:pPr><w:jc w:val=\"center\"/></w:pPr><w:bookmarkStart w:id=\"0\" w:name=\"x\"/>\
                <w:r><w:rPr><w:rFonts w:ascii=\"Arial\"/><w:sz w:val=\"28\"/></w:rPr><w:t>old</w:t></w:r><w:bookmarkEnd w:id=\"0\"/></w:p>\
                <w:sectPr><w:pgSz w:w=\"11906\"/></w:sectPr>";
    let (edited, _) = edit_docx(body, json!([{"find": "old", "replace": "new"}])).unwrap();
    let xml = body_of(&edited);
    let expected = format!("<w:body>{body}</w:body>").replace("<w:t>old</w:t>", "<w:t>new</w:t>");
    assert!(xml.contains(&expected), "{xml}");
}

#[test]
fn a_matching_phrase_cannot_cross_a_paragraph() {
    let body = "<w:p><w:r><w:t>first</w:t></w:r></w:p><w:p><w:r><w:t>second</w:t></w:r></w:p>";
    assert!(edit_docx(body, json!([{"find": "firstsecond", "replace": "x"}])).is_err());
}

#[test]
fn line_breaks_in_find_or_replace_and_missing_fields_are_refused() {
    let body = "<w:p><w:r><w:t>text</w:t></w:r></w:p>";
    for edits in [
        json!([{"find": "a\nb", "replace": "x"}]),
        json!([{"find": "text", "replace": "a\nb"}]),
        json!([{"find": "text", "replace": "a\rb"}]),
        json!([{"find": "", "replace": "x"}]),
        json!([{"replace": "x"}]),
        json!([{"find": "text"}]),
    ] {
        assert!(edit_docx(body, edits.clone()).is_err(), "{edits}");
    }
}

#[test]
fn the_edit_list_must_exist_and_stay_within_the_limit() {
    let original = docx("<w:p><w:r><w:t>text</w:t></w:r></w:p>");
    assert!(edit("a.docx", &original, &json!({"path": "a.docx"})).is_err());
    assert!(run("a.docx", &original, json!([])).is_err());
    let many: Vec<Value> = (0..=MAX_EDITS)
        .map(|_| json!({"find": "text", "replace": "text"}))
        .collect();
    let error = run("a.docx", &original, Value::Array(many)).unwrap_err();
    assert!(error.contains("at most"), "{error}");
}

#[test]
fn a_path_outside_the_workspace_and_unsupported_files_are_refused() {
    let original = docx("<w:p><w:r><w:t>text</w:t></w:r></w:p>");
    let edits = json!([{"find": "text", "replace": "x"}]);
    for path in ["../a.docx", "/etc/a.docx", ""] {
        assert!(run(path, &original, edits.clone()).is_err(), "{path}");
    }
    let pdf = run("a.pdf", &original, edits.clone()).unwrap_err();
    assert!(pdf.contains("PDF"), "{pdf}");
    let old = run("a.doc", &original, edits.clone()).unwrap_err();
    assert!(old.contains("old .doc"), "{old}");
    assert!(run("a.txt", &original, edits).is_err());
}

#[test]
fn a_file_that_is_not_an_office_document_is_refused() {
    let error = run(
        "a.docx",
        b"not a zip",
        json!([{"find": "a", "replace": "b"}]),
    )
    .unwrap_err();
    assert!(error.contains("not a valid Office document"), "{error}");
    let no_body = zip_of(&[("word/styles.xml", STYLES)]);
    assert!(run("a.docx", &no_body, json!([{"find": "a", "replace": "b"}])).is_err());
}

#[test]
fn a_replacement_without_changes_still_verifies_and_reports_the_match() {
    let body = "<w:p><w:r><w:t>same</w:t></w:r></w:p>";
    let (edited, summary) = edit_docx(body, json!([{"find": "same", "replace": "same"}])).unwrap();
    assert_eq!(docx_text(&edited), "same");
    assert_eq!(summary["edits"][0]["replaced"], 1);
}

// -- XLSX ------------------------------------------------------------------------------------

const SS: &str = "xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"";
const SHARED: &str = "<sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><si><t>Item</t></si><si><t>Qty</t></si><si><t>Widget</t></si></sst>";

fn xlsx_with(sheet1: &str, other: &str) -> Vec<u8> {
    zip_of(&[
        (
            "xl/workbook.xml",
            &format!("<workbook {SS}><sheets><sheet name=\"Data\" sheetId=\"1\" r:id=\"rId1\"/><sheet name=\"Other\" sheetId=\"2\" r:id=\"rId2\"/></sheets></workbook>"),
        ),
        (
            "xl/_rels/workbook.xml.rels",
            "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
             <Relationship Id=\"rId1\" Type=\"x\" Target=\"worksheets/sheet1.xml\"/>\
             <Relationship Id=\"rId2\" Type=\"x\" Target=\"worksheets/sheet2.xml\"/></Relationships>",
        ),
        ("xl/sharedStrings.xml", SHARED),
        ("xl/worksheets/sheet1.xml", sheet1),
        ("xl/worksheets/sheet2.xml", other),
    ])
}

const OTHER_SHEET: &str = "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData><row r=\"1\"><c r=\"A1\"><v>9</v></c></row></sheetData></worksheet>";

fn sheet(rows: &str) -> String {
    format!("<worksheet {SS}><sheetData>{rows}</sheetData><mergeCells count=\"1\"><mergeCell ref=\"A1:B1\"/></mergeCells></worksheet>")
}

fn edit_xlsx(rows: &str, edits: Value) -> Result<(Vec<u8>, Value), String> {
    run("a.xlsx", &xlsx_with(&sheet(rows), OTHER_SHEET), edits)
}

fn sheet_xml(bytes: &[u8]) -> String {
    part(bytes, "xl/worksheets/sheet1.xml")
}

fn xlsx_lines(bytes: &[u8]) -> Vec<String> {
    parse(Kind::Xlsx, bytes).unwrap().blocks[0]
        .text
        .lines()
        .map(str::to_owned)
        .collect()
}

const TABLE: &str = "<row r=\"1\"><c r=\"A1\" s=\"4\" t=\"s\"><v>0</v></c><c r=\"B1\" s=\"4\" t=\"s\"><v>1</v></c></row>\
                     <row r=\"2\"><c r=\"A2\" s=\"7\" t=\"s\"><v>2</v></c><c r=\"B2\" s=\"9\"><v>3</v></c></row>\
                     <row r=\"3\"><c r=\"B3\"><f>SUM(B2:B2)</f><v>3</v></c></row>";

#[test]
fn a_number_replaces_the_value_of_a_cell_and_the_cell_keeps_its_style() {
    let (edited, summary) =
        edit_xlsx(TABLE, json!([{"sheet": "Data", "cell": "B2", "value": 42}])).unwrap();
    let xml = sheet_xml(&edited);
    assert!(xml.contains("<c r=\"B2\" s=\"9\"><v>42</v></c>"), "{xml}");
    assert_eq!(summary["edits"][0]["cells"], 1);
    assert_eq!(xlsx_lines(&edited)[1], "2: Widget | 42");
}

#[test]
fn text_becomes_an_inline_string_and_the_shared_strings_are_untouched() {
    let (edited, _) = edit_xlsx(
        TABLE,
        json!([{"sheet": "Data", "cell": "A2", "value": "Gadget & Co"}]),
    )
    .unwrap();
    let xml = sheet_xml(&edited);
    assert!(
        xml.contains("<c r=\"A2\" s=\"7\" t=\"inlineStr\"><is><t>Gadget &amp; Co</t></is></c>"),
        "{xml}"
    );
    assert_eq!(part(&edited, "xl/sharedStrings.xml"), SHARED);
    assert_eq!(xlsx_lines(&edited)[1], "2: Gadget & Co | 3");
}

#[test]
fn text_with_edge_spaces_keeps_them() {
    let (edited, _) = edit_xlsx(
        TABLE,
        json!([{"sheet": "Data", "cell": "A2", "value": " padded "}]),
    )
    .unwrap();
    assert!(sheet_xml(&edited).contains("<t xml:space=\"preserve\"> padded </t>"));
}

#[test]
fn a_formula_is_written_as_a_formula_and_other_formulas_stay() {
    let (edited, _) = edit_xlsx(
        TABLE,
        json!([{"sheet": "Data", "cell": "C2", "value": "=B2*2"}]),
    )
    .unwrap();
    let xml = sheet_xml(&edited);
    assert!(xml.contains("<c r=\"C2\"><f>B2*2</f></c>"), "{xml}");
    assert!(
        xml.contains("<c r=\"B3\"><f>SUM(B2:B2)</f><v>3</v></c>"),
        "the other formula stays: {xml}"
    );
    assert_eq!(xlsx_lines(&edited)[1], "2: Widget | 3 | =B2*2");
}

#[test]
fn writing_a_plain_value_over_a_formula_replaces_it() {
    let (edited, _) =
        edit_xlsx(TABLE, json!([{"sheet": "Data", "cell": "B3", "value": 10}])).unwrap();
    let xml = sheet_xml(&edited);
    assert!(xml.contains("<c r=\"B3\"><v>10</v></c>"), "{xml}");
    assert!(!xml.contains("SUM(B2:B2)"));
}

#[test]
fn true_false_and_null_write_a_boolean_and_clear_a_cell_but_keep_its_style() {
    let (edited, _) = edit_xlsx(
        TABLE,
        json!([
            {"sheet": "Data", "cell": "C1", "value": true},
            {"sheet": "Data", "cell": "D1", "value": false},
            {"sheet": "Data", "cell": "B2", "value": null},
        ]),
    )
    .unwrap();
    let xml = sheet_xml(&edited);
    assert!(xml.contains("<c r=\"C1\" t=\"b\"><v>1</v></c>"), "{xml}");
    assert!(xml.contains("<c r=\"D1\" t=\"b\"><v>0</v></c>"), "{xml}");
    assert!(xml.contains("<c r=\"B2\" s=\"9\"/>"), "{xml}");
}

#[test]
fn a_new_cell_goes_into_its_row_in_column_order() {
    let (edited, _) = edit_xlsx(
        "<row r=\"1\"><c r=\"A1\"><v>1</v></c><c r=\"D1\"><v>4</v></c></row>",
        json!([{"sheet": "Data", "cell": "C1", "value": 3}, {"sheet": "Data", "cell": "F1", "value": 6}, {"sheet": "Data", "cell": "B1", "value": 2}]),
    )
    .unwrap();
    let xml = sheet_xml(&edited);
    let order: Vec<usize> = ["A1", "B1", "C1", "D1", "F1"]
        .iter()
        .map(|cell| xml.find(&format!("r=\"{cell}\"")).unwrap())
        .collect();
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{xml}");
}

#[test]
fn a_new_row_goes_in_row_order_and_an_empty_sheet_data_is_filled() {
    let (edited, _) = edit_xlsx(
        "<row r=\"1\"><c r=\"A1\"><v>1</v></c></row><row r=\"5\"><c r=\"A5\"><v>5</v></c></row>",
        json!([{"sheet": "Data", "cell": "A3", "value": 3}, {"sheet": "Data", "cell": "A9", "value": 9}, {"sheet": "Data", "cell": "A1", "value": 0}]),
    )
    .unwrap();
    let lines = xlsx_lines(&edited);
    assert_eq!(lines, ["1: 0", "3: 3", "5: 5", "9: 9"]);

    let empty = format!("<worksheet {SS}><sheetData/></worksheet>");
    let (filled, _) = run(
        "a.xlsx",
        &xlsx_with(&empty, OTHER_SHEET),
        json!([{"sheet": "Data", "cell": "B2", "value": "hi"}]),
    )
    .unwrap();
    assert_eq!(xlsx_lines(&filled), ["2:  | hi"]);
}

#[test]
fn a_range_writes_every_cell_and_a_wrong_size_is_refused() {
    let (edited, summary) = edit_xlsx(
        TABLE,
        json!([{"sheet": "Data", "range": "A5:B6", "values": [["a", 1], ["b", 2]]}]),
    )
    .unwrap();
    assert_eq!(summary["edits"][0]["cells"], 4);
    let lines = xlsx_lines(&edited);
    assert!(lines.contains(&"5: a | 1".to_owned()) && lines.contains(&"6: b | 2".to_owned()));
    for values in [
        json!([["a", 1]]),
        json!([["a"], ["b"]]),
        json!([["a", 1, 2], ["b", 2, 3]]),
    ] {
        let error = edit_xlsx(
            TABLE,
            json!([{"sheet": "Data", "range": "A5:B6", "values": values}]),
        )
        .unwrap_err();
        assert!(
            error.contains("rows") || error.contains("columns"),
            "{error}"
        );
    }
}

#[test]
fn a_range_given_backwards_is_the_same_range() {
    let (edited, _) = edit_xlsx(
        "<row r=\"1\"/>",
        json!([{"sheet": "Data", "range": "B2:A1", "values": [[1, 2], [3, 4]]}]),
    )
    .unwrap();
    assert_eq!(xlsx_lines(&edited), ["1: 1 | 2", "2: 3 | 4"]);
}

#[test]
fn rows_cells_sheets_and_merged_cells_the_edit_did_not_name_are_unchanged() {
    let original = xlsx_with(&sheet(TABLE), OTHER_SHEET);
    let (edited, _) = run(
        "a.xlsx",
        &original,
        json!([{"sheet": "Data", "cell": "B2", "value": 1}]),
    )
    .unwrap();
    let xml = sheet_xml(&edited);
    assert!(xml.contains("<mergeCells count=\"1\"><mergeCell ref=\"A1:B1\"/></mergeCells>"));
    assert!(xml.contains("<row r=\"1\"><c r=\"A1\" s=\"4\" t=\"s\"><v>0</v></c><c r=\"B1\" s=\"4\" t=\"s\"><v>1</v></c></row>"));
    assert_eq!(part(&edited, "xl/worksheets/sheet2.xml"), OTHER_SHEET);
    assert_eq!(
        part(&edited, "xl/workbook.xml"),
        part(&original, "xl/workbook.xml")
    );
}

#[test]
fn a_cell_in_another_sheet_is_found_by_the_sheet_name() {
    let (edited, summary) = edit_xlsx(
        TABLE,
        json!([{"sheet": "Other", "cell": "A1", "value": 10}]),
    )
    .unwrap();
    assert_eq!(
        part(&edited, "xl/worksheets/sheet2.xml")
            .matches("<v>10</v>")
            .count(),
        1
    );
    assert_eq!(part(&edited, "xl/worksheets/sheet1.xml"), sheet(TABLE));
    assert_eq!(summary["edits"][0]["sheet"], "Other");
}

#[test]
fn a_modified_row_drops_its_spans_hint() {
    let (edited, _) = edit_xlsx(
        "<row r=\"1\" spans=\"1:2\" ht=\"20\"><c r=\"A1\"><v>1</v></c></row>",
        json!([{"sheet": "Data", "cell": "E1", "value": 5}]),
    )
    .unwrap();
    let xml = sheet_xml(&edited);
    assert!(xml.contains("<row r=\"1\" ht=\"20\">"), "{xml}");
}

#[test]
fn a_worksheet_that_uses_a_namespace_prefix_is_edited_with_the_same_prefix() {
    let prefixed = "<x:worksheet xmlns:x=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><x:sheetData>\
                    <x:row r=\"1\"><x:c r=\"A1\"><x:v>1</x:v></x:c></x:row></x:sheetData></x:worksheet>";
    let (edited, _) = run(
        "a.xlsx",
        &xlsx_with(prefixed, OTHER_SHEET),
        json!([{"sheet": "Data", "cell": "B1", "value": "two"}, {"sheet": "Data", "cell": "A2", "value": 3}]),
    )
    .unwrap();
    let xml = sheet_xml(&edited);
    assert!(
        xml.contains("<x:c r=\"B1\" t=\"inlineStr\"><x:is><x:t>two</x:t></x:is></x:c>"),
        "{xml}"
    );
    assert!(
        xml.contains("<x:row r=\"2\"><x:c r=\"A2\"><x:v>3</x:v></x:c></x:row>"),
        "{xml}"
    );
    assert_eq!(xlsx_lines(&edited), ["1: 1 | two", "2: 3"]);
}

#[test]
fn bad_sheet_cell_and_value_arguments_are_refused() {
    for edits in [
        json!([{"sheet": "Nope", "cell": "A1", "value": 1}]),
        json!([{"cell": "A1", "value": 1}]),
        json!([{"sheet": "Data", "cell": "A", "value": 1}]),
        json!([{"sheet": "Data", "cell": "1A", "value": 1}]),
        json!([{"sheet": "Data", "cell": "A0", "value": 1}]),
        json!([{"sheet": "Data", "cell": "ABCD1", "value": 1}]),
        json!([{"sheet": "Data", "cell": "XFE1", "value": 1}]),
        json!([{"sheet": "Data", "cell": "A1048577", "value": 1}]),
        json!([{"sheet": "Data", "cell": "A1"}]),
        json!([{"sheet": "Data", "cell": "A1", "value": {"a": 1}}]),
        json!([{"sheet": "Data", "cell": "A1", "value": [1]}]),
        json!([{"sheet": "Data", "cell": "A1", "range": "A1:A1", "value": 1}]),
        json!([{"sheet": "Data", "value": 1}]),
        json!([{"sheet": "Data", "range": "A1", "values": [[1]]}]),
        json!([{"sheet": "Data", "range": "A1:B1"}]),
    ] {
        assert!(edit_xlsx(TABLE, edits.clone()).is_err(), "{edits}");
    }
}

#[test]
fn the_last_column_and_row_are_valid_and_a_long_text_is_refused() {
    let (edited, _) = edit_xlsx(
        TABLE,
        json!([{"sheet": "Data", "cell": "XFD1048576", "value": 1}]),
    )
    .unwrap();
    assert!(sheet_xml(&edited).contains("<c r=\"XFD1048576\">"));
    let long = "x".repeat(MAX_CHARS + 1);
    assert!(edit_xlsx(
        TABLE,
        json!([{"sheet": "Data", "cell": "A1", "value": long}])
    )
    .is_err());
}

#[test]
fn a_single_equals_sign_is_text_not_a_formula() {
    let (edited, _) = edit_xlsx(
        TABLE,
        json!([{"sheet": "Data", "cell": "C1", "value": "="}]),
    )
    .unwrap();
    assert!(sheet_xml(&edited).contains("<c r=\"C1\" t=\"inlineStr\"><is><t>=</t></is></c>"));
}

// -- PPTX ------------------------------------------------------------------------------------

const A: &str = "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"";
const NOTES: &str = "<p:notes xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><a:p><a:r><a:t>Quarterly results</a:t></a:r></a:p></p:notes>";

fn slide(runs: &str) -> String {
    format!("<p:sld {A}><a:p><a:r><a:rPr lang=\"en-US\" b=\"1\"/>{runs}</a:r></a:p></p:sld>")
}

fn pptx() -> Vec<u8> {
    zip_of(&[
        (
            "ppt/slides/slide1.xml",
            &slide("<a:t>Quarterly results</a:t>"),
        ),
        (
            "ppt/slides/slide2.xml",
            &slide("<a:t>Quarterly results</a:t>"),
        ),
        ("ppt/slides/slide10.xml", &slide("<a:t>Appendix</a:t>")),
        ("ppt/notesSlides/notesSlide1.xml", NOTES),
    ])
}

fn pptx_text(bytes: &[u8], slide: usize) -> String {
    parse(Kind::Pptx, bytes).unwrap().blocks[slide - 1]
        .text
        .clone()
}

#[test]
fn text_is_replaced_on_the_named_slide_only() {
    let (edited, summary) = run(
        "a.pptx",
        &pptx(),
        json!([{"slide": 2, "find": "Quarterly", "replace": "Annual"}]),
    )
    .unwrap();
    assert_eq!(
        pptx_text(&edited, 1),
        "Quarterly results\n\nNotes:\nQuarterly results"
    );
    assert_eq!(pptx_text(&edited, 2), "Annual results");
    assert_eq!(summary["edits"][0]["replaced"], 1);
}

#[test]
fn slides_are_numbered_the_way_read_document_shows_them() {
    let (edited, _) = run(
        "a.pptx",
        &pptx(),
        json!([{"slide": 3, "find": "Appendix", "replace": "Annex"}]),
    )
    .unwrap();
    assert_eq!(pptx_text(&edited, 3), "Annex");
    assert!(part(&edited, "ppt/slides/slide10.xml").contains("Annex"));
}

#[test]
fn the_run_formatting_of_a_slide_survives_the_edit() {
    let (edited, _) = run(
        "a.pptx",
        &pptx(),
        json!([{"slide": 1, "find": "results", "replace": "numbers"}]),
    )
    .unwrap();
    assert!(part(&edited, "ppt/slides/slide1.xml")
        .contains("<a:rPr lang=\"en-US\" b=\"1\"/><a:t>Quarterly numbers</a:t>"));
}

#[test]
fn speaker_notes_and_the_other_slides_are_copied_unchanged() {
    let (edited, _) = run(
        "a.pptx",
        &pptx(),
        json!([{"slide": 1, "find": "Quarterly", "replace": "Monthly"}]),
    )
    .unwrap();
    assert_eq!(part(&edited, "ppt/notesSlides/notesSlide1.xml"), NOTES);
    assert_eq!(
        part(&edited, "ppt/slides/slide2.xml"),
        slide("<a:t>Quarterly results</a:t>")
    );
}

#[test]
fn edits_on_different_slides_are_each_reported() {
    let (edited, summary) = run(
        "a.pptx",
        &pptx(),
        json!([
            {"slide": 2, "find": "results", "replace": "plan"},
            {"slide": 1, "find": "results", "replace": "recap"},
            {"slide": 2, "find": "Quarterly", "replace": "Annual"},
        ]),
    )
    .unwrap();
    assert_eq!(pptx_text(&edited, 2), "Annual plan");
    assert!(pptx_text(&edited, 1).starts_with("Quarterly recap"));
    let reported: Vec<u64> = summary["edits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|edit| edit["edit"].as_u64().unwrap())
        .collect();
    assert_eq!(reported, [1, 2, 3]);
}

#[test]
fn a_missing_or_unknown_slide_is_refused() {
    for edits in [
        json!([{"find": "Quarterly", "replace": "x"}]),
        json!([{"slide": 0, "find": "Quarterly", "replace": "x"}]),
        json!([{"slide": 4, "find": "Quarterly", "replace": "x"}]),
        json!([{"slide": "two", "find": "Quarterly", "replace": "x"}]),
    ] {
        assert!(run("a.pptx", &pptx(), edits.clone()).is_err(), "{edits}");
    }
    let ambiguous = run(
        "a.pptx",
        &zip_of(&[("ppt/slides/slide1.xml", &slide("<a:t>x x</a:t>"))]),
        json!([{"slide": 1, "find": "x", "replace": "y"}]),
    )
    .unwrap_err();
    assert!(ambiguous.contains("appears 2 times"), "{ambiguous}");
}

// -- the tool --------------------------------------------------------------------------------

#[test]
fn the_tool_is_registered_with_a_declared_side_effect_and_approval() {
    let registration = edit_registration();
    assert_eq!(registration.tool_name, "edit_document");
    assert_eq!(
        registration.side_effect,
        crate::skills::SideEffectClass::LocalWrite
    );
    assert!(registration.workspace_write);
    assert_eq!(registration.metadata["requires_approval"], true);
}

// -- real files ------------------------------------------------------------------------------
//
// These three files were made by python-docx, openpyxl and python-pptx, so they carry what real
// documents carry (the default templates, numbering, a header, conditional formatting, notes).
// The edited files were also opened in LibreOffice and read back with the same libraries when the
// fixtures were added.

const REAL_DOCX: &[u8] = include_bytes!("../../../tests/fixtures/office/report1.docx");
const REAL_XLSX: &[u8] = include_bytes!("../../../tests/fixtures/office/book1.xlsx");
const REAL_PPTX: &[u8] = include_bytes!("../../../tests/fixtures/office/deck1.pptx");

/// Every part of `original` except the named ones comes through byte for byte.
fn assert_other_parts_unchanged(original: &[u8], edited: &[u8], changed: &[&str]) {
    let mut names: Vec<String> = ooxml::open(original)
        .unwrap()
        .file_names()
        .map(str::to_owned)
        .collect();
    names.sort();
    let mut after: Vec<String> = ooxml::open(edited)
        .unwrap()
        .file_names()
        .map(str::to_owned)
        .collect();
    after.sort();
    assert_eq!(names, after, "the same parts");
    for name in names
        .iter()
        .filter(|name| !changed.contains(&name.as_str()))
    {
        let read = |bytes: &[u8]| {
            let mut archive = ooxml::open(bytes).unwrap();
            ooxml::required_part(&mut archive, name).unwrap()
        };
        assert!(read(original) == read(edited), "{name} changed");
    }
}

#[test]
fn a_real_docx_keeps_its_styles_header_numbering_and_table() {
    let (edited, summary) = run(
        "report1.docx",
        REAL_DOCX,
        json!([
            {"find": "Q3 2026", "replace": "Q4 2026"},
            {"find": "Second point about Q3", "replace": "Second point about Q4"},
            {"find": "Q3: 10", "replace": "Q4: 12"},
        ]),
    )
    .unwrap();
    assert_eq!(summary["edits"].as_array().unwrap().len(), 3);
    assert_other_parts_unchanged(REAL_DOCX, &edited, &["word/document.xml"]);
    let text = docx_text(&edited);
    assert!(
        text.contains("Revenue grew in Q4 2026 across all regions."),
        "{text}"
    );
    assert!(text.contains("Region | Revenue\nEast | Q4: 12"), "{text}");
    let xml = body_of(&edited);
    let at = xml.find("Q4 2026").unwrap();
    assert!(
        xml[..at].rsplit("<w:r>").next().unwrap().contains("<w:i/>"),
        "the italic run keeps its formatting"
    );
    assert!(xml.contains("w:val=\"ListBullet\"") && xml.contains("w:val=\"Heading1\""));
}

#[test]
fn a_real_xlsx_keeps_its_formulas_styles_merges_and_other_sheet_parts() {
    let (edited, _) = run(
        "book1.xlsx",
        REAL_XLSX,
        json!([
            {"sheet": "Data", "cell": "B2", "value": 10},
            {"sheet": "Data", "cell": "A5", "value": "Doohickey"},
            {"sheet": "Data", "cell": "D5", "value": "=B5*C5"},
            {"sheet": "Data", "range": "A10:B11", "values": [["new", "x"], [1, 2]]},
            {"sheet": "Other", "cell": "B1", "value": 8},
        ]),
    )
    .unwrap();
    assert_other_parts_unchanged(
        REAL_XLSX,
        &edited,
        &["xl/worksheets/sheet1.xml", "xl/worksheets/sheet2.xml"],
    );
    let xml = sheet_xml(&edited);
    for kept in [
        "<f>B2*C2</f>",
        "<f>SUM(D2:D4)</f>",
        "<mergeCell ref=\"A8:D8\"/>",
        "<conditionalFormatting",
        "<cols>",
    ] {
        assert!(xml.contains(kept), "{kept} is gone: {xml}");
    }
    assert!(xml.contains("<f>B5*C5</f>"));
    let lines = xlsx_lines(&edited);
    assert!(lines[1].starts_with("2: Widget | 10 | 2.5"), "{lines:?}");
    assert!(
        lines.contains(&"5: Doohickey |  |  | =B5*C5".to_owned()),
        "{lines:?}"
    );
    assert!(
        lines.contains(&"10: new | x".to_owned()) && lines.contains(&"11: 1 | 2".to_owned()),
        "{lines:?}"
    );
}

#[test]
fn a_real_pptx_keeps_its_layouts_run_formatting_and_notes() {
    let (edited, _) = run(
        "deck1.pptx",
        REAL_PPTX,
        json!([
            {"slide": 1, "find": "Revenue up in Q3", "replace": "Revenue up in Q4"},
            {"slide": 1, "find": "Margin steady at 40%", "replace": "Margin up to 42%"},
            {"slide": 2, "find": "Q4 expected to be stronger", "replace": "Q1 expected to be stronger"},
        ]),
    )
    .unwrap();
    assert_other_parts_unchanged(
        REAL_PPTX,
        &edited,
        &["ppt/slides/slide1.xml", "ppt/slides/slide2.xml"],
    );
    assert!(pptx_text(&edited, 1).contains("Margin up to 42%"));
    assert!(
        pptx_text(&edited, 1).ends_with("Notes:\nMention Q3 explicitly"),
        "{}",
        pptx_text(&edited, 1)
    );
    assert!(pptx_text(&edited, 2).contains("Q1 expected to be stronger"));
    let xml = part(&edited, "ppt/slides/slide1.xml");
    let at = xml.find("Margin up to 42%").unwrap();
    assert!(
        xml[..at]
            .rsplit("<a:r>")
            .next()
            .unwrap()
            .contains("b=\"1\""),
        "the bold run keeps its formatting"
    );
}
