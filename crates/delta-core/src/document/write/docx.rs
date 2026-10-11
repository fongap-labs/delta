//! DOCX: a title, headings, paragraphs, bullet and numbered lists, tables and page breaks.

use serde_json::Value;

use super::{array, esc, package, text_of, XML};

const MAX_BLOCKS: usize = 5_000;
const MAX_TABLE_COLUMNS: usize = 30;
/// Usable page width in twentieths of a point (A4 with 2.54 cm margins).
const PAGE_WIDTH: usize = 9026;

const NS: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
    xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"";

fn run(text: &str, bold: bool) -> String {
    let props = if bold { "<w:rPr><w:b/></w:rPr>" } else { "" };
    // `xml:space` keeps leading and trailing spaces; tabs and newlines become real breaks.
    let mut out = String::new();
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push_str(&format!("<w:r>{props}<w:br/></w:r>"));
        }
        for (j, piece) in line.split('\t').enumerate() {
            if j > 0 {
                out.push_str(&format!("<w:r>{props}<w:tab/></w:r>"));
            }
            if !piece.is_empty() {
                out.push_str(&format!(
                    "<w:r>{props}<w:t xml:space=\"preserve\">{}</w:t></w:r>",
                    esc(piece)
                ));
            }
        }
    }
    out
}

fn paragraph(style: Option<&str>, num: Option<usize>, text: &str, bold: bool) -> String {
    let mut props = String::new();
    if let Some(style) = style {
        props.push_str(&format!("<w:pStyle w:val=\"{style}\"/>"));
    }
    if let Some(num) = num {
        props.push_str(&format!(
            "<w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"{num}\"/></w:numPr>"
        ));
    }
    let props = if props.is_empty() {
        String::new()
    } else {
        format!("<w:pPr>{props}</w:pPr>")
    };
    format!("<w:p>{props}{}</w:p>", run(text, bold))
}

fn table(block: &Value) -> Result<String, String> {
    let rows = array(block, "rows")?;
    let header: Vec<String> = match block.get("header") {
        Some(Value::Array(cells)) => cells
            .iter()
            .map(|cell| text_of(cell, "a table header cell"))
            .collect::<Result<_, _>>()?,
        _ => Vec::new(),
    };
    let mut grid: Vec<(Vec<String>, bool)> = Vec::new();
    if !header.is_empty() {
        grid.push((header, true));
    }
    for row in rows {
        let cells = row
            .as_array()
            .ok_or_else(|| "each table row must be a list of cells".to_owned())?;
        let texts = cells
            .iter()
            .map(|cell| text_of(cell, "a table cell"))
            .collect::<Result<Vec<_>, _>>()?;
        grid.push((texts, false));
    }
    let columns = grid.iter().map(|(cells, _)| cells.len()).max().unwrap_or(0);
    if columns == 0 {
        return Err("a table needs at least one cell".to_owned());
    }
    if columns > MAX_TABLE_COLUMNS {
        return Err(format!(
            "a table may have at most {MAX_TABLE_COLUMNS} columns"
        ));
    }
    let width = PAGE_WIDTH / columns;
    let mut xml = String::from(
        "<w:tbl><w:tblPr><w:tblStyle w:val=\"TableGrid\"/><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid>",
    );
    for _ in 0..columns {
        xml.push_str(&format!("<w:gridCol w:w=\"{width}\"/>"));
    }
    xml.push_str("</w:tblGrid>");
    for (cells, is_header) in &grid {
        xml.push_str("<w:tr>");
        if *is_header {
            // A header row repeats at the top of every page the table spans.
            xml.push_str("<w:trPr><w:tblHeader/></w:trPr>");
        }
        for column in 0..columns {
            let text = cells.get(column).map(String::as_str).unwrap_or("");
            let shading = if *is_header {
                "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"D9E2F3\"/>"
            } else {
                ""
            };
            xml.push_str(&format!(
                "<w:tc><w:tcPr><w:tcW w:w=\"{width}\" w:type=\"dxa\"/>{shading}</w:tcPr>{}</w:tc>",
                paragraph(None, None, text, *is_header)
            ));
        }
        xml.push_str("</w:tr>");
    }
    xml.push_str("</w:tbl>");
    // Word requires a paragraph after a table that ends the body or precedes another table.
    xml.push_str("<w:p/>");
    Ok(xml)
}

pub fn build(arguments: &Value) -> Result<Vec<u8>, String> {
    let blocks = array(arguments, "blocks")?;
    if blocks.is_empty()
        && arguments
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .is_empty()
    {
        return Err("blocks must not be empty".to_owned());
    }
    if blocks.len() > MAX_BLOCKS {
        return Err(format!("a document may have at most {MAX_BLOCKS} blocks"));
    }
    let mut body = String::new();
    if let Some(title) = arguments
        .get("title")
        .and_then(Value::as_str)
        .filter(|t| !t.trim().is_empty())
    {
        body.push_str(&paragraph(Some("Title"), None, title, false));
    }
    // Each numbered list restarts at 1: its own `num` pointing at the numbered abstract list.
    let mut numbered_lists = 0usize;
    for (index, block) in blocks.iter().enumerate() {
        let kind = block
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("paragraph");
        let at = |what: &str| format!("block {} ({kind}): {what}", index + 1);
        match kind {
            "heading" => {
                let level = block.get("level").and_then(Value::as_i64).unwrap_or(1).clamp(1, 3);
                let text = text_of(block.get("text").unwrap_or(&Value::Null), "a heading").map_err(|e| at(&e))?;
                body.push_str(&paragraph(Some(&format!("Heading{level}")), None, &text, false));
            }
            "paragraph" => {
                let text = text_of(block.get("text").unwrap_or(&Value::Null), "a paragraph").map_err(|e| at(&e))?;
                body.push_str(&paragraph(None, None, &text, false));
            }
            "bullets" | "numbered" => {
                let items = array(block, "items").map_err(|e| at(&e))?;
                let num = if kind == "bullets" {
                    1
                } else {
                    numbered_lists += 1;
                    1 + numbered_lists
                };
                for item in items {
                    let text = text_of(item, "a list item").map_err(|e| at(&e))?;
                    body.push_str(&paragraph(Some("ListParagraph"), Some(num), &text, false));
                }
            }
            "table" => body.push_str(&table(block).map_err(|e| at(&e))?),
            "page_break" => body.push_str("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>"),
            other => return Err(at(&format!("unknown block type {other:?}; use heading, paragraph, bullets, numbered, table or page_break"))),
        }
    }
    let document = format!(
        "{XML}<w:document {NS}><w:body>{body}<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/>\
         <w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\" w:header=\"720\" w:footer=\"720\" w:gutter=\"0\"/>\
         </w:sectPr></w:body></w:document>"
    );

    let mut numbering = format!(
        "{XML}<w:numbering {NS}>\
         <w:abstractNum w:abstractNumId=\"0\"><w:multiLevelType w:val=\"hybridMultilevel\"/>\
         <w:lvl w:ilvl=\"0\"><w:start w:val=\"1\"/><w:numFmt w:val=\"bullet\"/><w:lvlText w:val=\"&#8226;\"/><w:lvlJc w:val=\"left\"/>\
         <w:pPr><w:ind w:left=\"720\" w:hanging=\"360\"/></w:pPr><w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\" w:hint=\"default\"/></w:rPr></w:lvl></w:abstractNum>\
         <w:abstractNum w:abstractNumId=\"1\"><w:multiLevelType w:val=\"hybridMultilevel\"/>\
         <w:lvl w:ilvl=\"0\"><w:start w:val=\"1\"/><w:numFmt w:val=\"decimal\"/><w:lvlText w:val=\"%1.\"/><w:lvlJc w:val=\"left\"/>\
         <w:pPr><w:ind w:left=\"720\" w:hanging=\"360\"/></w:pPr></w:lvl></w:abstractNum>\
         <w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num>"
    );
    for list in 0..numbered_lists {
        numbering.push_str(&format!(
            "<w:num w:numId=\"{}\"><w:abstractNumId w:val=\"1\"/><w:lvlOverride w:ilvl=\"0\"><w:startOverride w:val=\"1\"/></w:lvlOverride></w:num>",
            2 + list
        ));
    }
    numbering.push_str("</w:numbering>");

    let heading = |id: &str, name: &str, size: u32, before: u32| {
        format!(
            "<w:style w:type=\"paragraph\" w:styleId=\"{id}\"><w:name w:val=\"{name}\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/>\
             <w:pPr><w:keepNext/><w:spacing w:before=\"{before}\" w:after=\"120\"/><w:outlineLvl w:val=\"{}\"/></w:pPr>\
             <w:rPr><w:b/><w:sz w:val=\"{size}\"/><w:szCs w:val=\"{size}\"/></w:rPr></w:style>",
            id.trim_start_matches("Heading").parse::<u32>().unwrap_or(1) - 1
        )
    };
    let styles = format!(
        "{XML}<w:styles {NS}>\
         <w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\" w:eastAsia=\"Microsoft YaHei\" w:cs=\"Calibri\"/>\
         <w:sz w:val=\"22\"/><w:szCs w:val=\"22\"/><w:lang w:val=\"en-US\" w:eastAsia=\"zh-CN\"/></w:rPr></w:rPrDefault>\
         <w:pPrDefault><w:pPr><w:spacing w:after=\"160\" w:line=\"300\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>\
         <w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/></w:style>\
         <w:style w:type=\"paragraph\" w:styleId=\"Title\"><w:name w:val=\"Title\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/>\
         <w:pPr><w:spacing w:after=\"240\"/></w:pPr><w:rPr><w:b/><w:sz w:val=\"48\"/><w:szCs w:val=\"48\"/></w:rPr></w:style>\
         {}{}{}\
         <w:style w:type=\"paragraph\" w:styleId=\"ListParagraph\"><w:name w:val=\"List Paragraph\"/><w:basedOn w:val=\"Normal\"/><w:qFormat/>\
         <w:pPr><w:spacing w:after=\"60\"/><w:ind w:left=\"720\"/></w:pPr></w:style>\
         <w:style w:type=\"table\" w:default=\"1\" w:styleId=\"TableNormal\"><w:name w:val=\"Normal Table\"/><w:uiPriority w:val=\"99\"/><w:semiHidden/>\
         <w:tblPr><w:tblInd w:w=\"0\" w:type=\"dxa\"/><w:tblCellMar><w:top w:w=\"0\" w:type=\"dxa\"/><w:left w:w=\"108\" w:type=\"dxa\"/><w:bottom w:w=\"0\" w:type=\"dxa\"/><w:right w:w=\"108\" w:type=\"dxa\"/></w:tblCellMar></w:tblPr></w:style>\
         <w:style w:type=\"table\" w:styleId=\"TableGrid\"><w:name w:val=\"Table Grid\"/><w:basedOn w:val=\"TableNormal\"/>\
         <w:tblPr><w:tblBorders><w:top w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"808080\"/><w:left w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"808080\"/>\
         <w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"808080\"/><w:right w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"808080\"/>\
         <w:insideH w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"808080\"/><w:insideV w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"808080\"/></w:tblBorders></w:tblPr></w:style>\
         </w:styles>",
        heading("Heading1", "heading 1", 36, 360),
        heading("Heading2", "heading 2", 30, 280),
        heading("Heading3", "heading 3", 26, 200),
    );

    package(vec![
        (
            "[Content_Types].xml".to_owned(),
            format!(
                "{XML}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
                 <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
                 <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
                 <Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
                 <Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>\
                 <Override PartName=\"/word/numbering.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml\"/>\
                 </Types>"
            ),
        ),
        (
            "_rels/.rels".to_owned(),
            format!(
                "{XML}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
                 <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
                 </Relationships>"
            ),
        ),
        (
            "word/_rels/document.xml.rels".to_owned(),
            format!(
                "{XML}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
                 <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>\
                 <Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering\" Target=\"numbering.xml\"/>\
                 </Relationships>"
            ),
        ),
        ("word/document.xml".to_owned(), document),
        ("word/styles.xml".to_owned(), styles),
        ("word/numbering.xml".to_owned(), numbering),
    ])
}
