//! Reading the documents office work is made of: PDF, DOCX, XLSX and PPTX (ADR-0054).
//!
//! `read_document` turns a file into blocks (a page, a sheet, a slide, a group of paragraphs) so
//! the model can look at the whole thing first and then ask for the part it needs. Reading is
//! read-only and runs under the same workspace grant as `read_file`; a read counts as a source read.

mod ooxml;
mod pdf;

use std::path::Path;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::capability::{
    CapabilityGrants, CapabilityRegistration, CapabilityResult, NativeCapabilityRunner,
};

/// Biggest file read, in bytes.
pub const MAX_FILE_BYTES: u64 = 100 * 1024 * 1024;
/// Characters returned for one block, or in total for the whole-document view.
pub const MAX_CHARS: usize = 20_000;
/// A DOCX is cut into blocks of about this many characters.
const DOCX_BLOCK_CHARS: usize = 3_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Pdf,
    Docx,
    Xlsx,
    Pptx,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Pdf => "pdf",
            Kind::Docx => "docx",
            Kind::Xlsx => "xlsx",
            Kind::Pptx => "pptx",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub index: usize,
    pub label: String,
    pub text: String,
    /// A PDF page with no text layer.
    pub scanned: bool,
    /// Extra facts for the summary (a sheet's row count).
    pub rows: Option<usize>,
}

#[derive(Debug)]
pub struct Document {
    pub kind: Kind,
    pub blocks: Vec<Block>,
}

pub fn kind_of(path: &Path) -> Result<Kind, String> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match extension.as_str() {
        "pdf" => Ok(Kind::Pdf),
        "docx" => Ok(Kind::Docx),
        "xlsx" | "xlsm" => Ok(Kind::Xlsx),
        "pptx" => Ok(Kind::Pptx),
        "doc" | "xls" | "ppt" => Err(format!(
            "this is an old .{extension} file; open it in Office and save it as .{extension}x first"
        )),
        _ => Err(format!(
            "unsupported file type .{extension}; read_document reads PDF, DOCX, XLSX and PPTX (use read_file for text)"
        )),
    }
}

fn first_line(text: &str, max: usize) -> String {
    let line = text
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .trim_start_matches('#')
        .trim();
    let mut clipped: String = line.chars().take(max).collect();
    if line.chars().count() > max {
        clipped.push('…');
    }
    clipped
}

pub fn parse(kind: Kind, bytes: &[u8]) -> Result<Document, String> {
    let blocks = match kind {
        Kind::Pdf => pdf::read_pdf(bytes)?
            .into_iter()
            .enumerate()
            .map(|(index, text)| {
                let scanned = text.is_empty();
                Block {
                    index,
                    label: format!("page {}", index + 1),
                    text: if scanned {
                        "(no text on this page; it is probably a scan)".to_owned()
                    } else {
                        text
                    },
                    scanned,
                    rows: None,
                }
            })
            .collect(),
        Kind::Docx => ooxml::pack_blocks(ooxml::read_docx(bytes)?, DOCX_BLOCK_CHARS)
            .into_iter()
            .enumerate()
            .map(|(index, lines)| {
                let text = lines.join("\n");
                Block {
                    index,
                    label: format!("part {}: {}", index + 1, first_line(&text, 40)),
                    text,
                    scanned: false,
                    rows: None,
                }
            })
            .collect(),
        Kind::Xlsx => ooxml::read_xlsx(bytes)?
            .into_iter()
            .enumerate()
            .map(|(index, sheet)| Block {
                index,
                label: format!("sheet '{}'", sheet.name),
                text: sheet.lines.join("\n"),
                scanned: false,
                rows: Some(sheet.row_count),
            })
            .collect(),
        Kind::Pptx => ooxml::read_pptx(bytes)?
            .into_iter()
            .enumerate()
            .map(|(index, slide)| {
                let mut text = slide.lines.join("\n");
                if !slide.notes.is_empty() {
                    text.push_str("\n\nNotes:\n");
                    text.push_str(&slide.notes.join("\n"));
                }
                Block {
                    index,
                    label: format!("slide {}: {}", index + 1, first_line(&text, 40)),
                    text,
                    scanned: false,
                    rows: None,
                }
            })
            .collect(),
    };
    Ok(Document { kind, blocks })
}

/// Read and parse a file, refusing anything over [`MAX_FILE_BYTES`].
pub fn read_file(path: &Path) -> Result<Document, String> {
    let kind = kind_of(path)?;
    let size = std::fs::metadata(path)
        .map_err(|error| format!("could not read the file: {error}"))?
        .len();
    if size > MAX_FILE_BYTES {
        return Err(format!("the file is too large to read ({size} bytes)"));
    }
    let bytes = std::fs::read(path).map_err(|error| format!("could not read the file: {error}"))?;
    let document = parse(kind, &bytes)?;
    if document.blocks.is_empty() || document.blocks.iter().all(|b| b.text.trim().is_empty()) {
        return Err(format!("the {} has no readable content", kind.as_str()));
    }
    Ok(document)
}

fn clip(text: &str, max: usize) -> (String, bool) {
    if text.chars().count() <= max {
        return (text.to_owned(), false);
    }
    (text.chars().take(max).collect(), true)
}

/// The JSON the model sees. With `block` it is that block's text; without, a listing of every
/// block plus the start of the document, so a short file needs one call.
pub fn describe(document: &Document, path: &str, block: Option<i64>) -> Result<Value, String> {
    let count = document.blocks.len();
    if let Some(wanted) = block {
        let chosen = usize::try_from(wanted)
            .ok()
            .and_then(|i| document.blocks.get(i))
            .ok_or_else(|| {
                format!(
                    "there is no block {wanted}; the document has {count} (0 to {})",
                    count - 1
                )
            })?;
        let (text, truncated) = clip(&chosen.text, MAX_CHARS);
        return Ok(json!({
            "ok": true, "path": path, "kind": document.kind.as_str(), "block_count": count,
            "block": {
                "index": chosen.index, "label": chosen.label, "text": text,
                "truncated": truncated, "scanned": chosen.scanned, "rows": chosen.rows,
            },
        }));
    }
    let listing: Vec<Value> = document
        .blocks
        .iter()
        .map(|b| json!({"index": b.index, "label": b.label, "scanned": b.scanned, "rows": b.rows}))
        .collect();
    let mut whole = String::new();
    for b in &document.blocks {
        if !whole.is_empty() {
            whole.push_str("\n\n");
        }
        whole.push_str(&format!("[{}]\n{}", b.label, b.text));
        if whole.chars().count() > MAX_CHARS {
            break;
        }
    }
    let (text, truncated) = clip(&whole, MAX_CHARS);
    Ok(json!({
        "ok": true, "path": path, "kind": document.kind.as_str(), "block_count": count,
        "blocks": listing, "text": text, "truncated": truncated,
    }))
}

pub fn read_registration() -> CapabilityRegistration {
    CapabilityRegistration {
        capability_id: "document.read".to_owned(),
        tool_name: "read_document".to_owned(),
        description: "Read a PDF, DOCX, XLSX or PPTX file in the workspace and return its text. \
            Without `block` you get the list of blocks (PDF pages, sheets, slides, groups of \
            paragraphs) and the start of the document; with a 0-based `block` you get that block in \
            full. Spreadsheet rows are `row: cell | cell`. Text, Markdown and CSV files are read \
            with read_file."
            .to_owned(),
        parameters: json!({
            "type": "object", "required": ["path"],
            "properties": {
                "path": {"type": "string", "description": "File path, relative to the workspace."},
                "block": {"type": "integer", "description": "0-based block to read in full."}
            }
        }),
        metadata: json!({
            "risk_level": "low", "requires_approval": false,
            "category": "read", "capabilities": ["document.read"]
        }),
        grants: CapabilityGrants::read_only(),
        workspace_write: false,
        runner: Arc::new(NativeCapabilityRunner::new(|job, control| {
            let Some(path) = job.arguments.get("path").and_then(Value::as_str) else {
                return CapabilityResult::failed(&job.job_id, "path is required", Some("arguments"));
            };
            let block = job.arguments.get("block").and_then(Value::as_i64);
            let resolved = match crate::capability::resolve_read_path(job, path) {
                Ok(resolved) => resolved,
                Err((code, message)) => return CapabilityResult::failed(&job.job_id, &message, Some(code)),
            };
            if control.is_cancelled() {
                return CapabilityResult::cancelled(&job.job_id);
            }
            match read_file(&resolved).and_then(|document| describe(&document, path, block)) {
                Ok(value) => CapabilityResult::completed(&job.job_id, value),
                Err(error) => CapabilityResult::failed(&job.job_id, &error, Some("read_failed")),
            }
        })),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    /// A ZIP of `(part name, content)` pairs, the container every OOXML file is.
    fn zip_of(parts: &[(&str, &str)]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut out));
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

    const W: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"";

    fn docx(body: &str) -> Vec<u8> {
        zip_of(&[(
            "word/document.xml",
            &format!("<?xml version=\"1.0\"?><w:document {W}><w:body>{body}</w:body></w:document>"),
        )])
    }

    #[test]
    fn docx_text_keeps_headings_tabs_breaks_entities_and_tables_in_order() {
        let bytes = docx(
            "<w:p><w:pPr><w:pStyle w:val=\"Heading2\"/></w:pPr><w:r><w:t>Scope &amp; goals</w:t></w:r></w:p>\
             <w:p><w:r><w:t xml:space=\"preserve\">one </w:t></w:r><w:r><w:t>line</w:t><w:tab/><w:t>x</w:t><w:br/><w:t>next</w:t></w:r>\
               <w:r><w:delText>deleted</w:delText></w:r></w:p>\
             <w:p/>\
             <w:tbl><w:tr><w:tc><w:p><w:r><w:t>Region</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Revenue</w:t></w:r></w:p></w:tc></w:tr>\
               <w:tr><w:tc><w:p><w:r><w:t>华东</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>1</w:t></w:r></w:p><w:p><w:r><w:t>2</w:t></w:r></w:p></w:tc></w:tr></w:tbl>\
             <w:p><w:r><w:t>after &#x4e2d;&#25991;</w:t></w:r></w:p>",
        );
        let document = parse(Kind::Docx, &bytes).unwrap();
        assert_eq!(document.blocks.len(), 1);
        assert_eq!(
            document.blocks[0].text,
            "## Scope & goals\none line\tx\nnext\nRegion | Revenue\n华东 | 1 2\nafter 中文"
        );
    }

    #[test]
    fn a_long_docx_is_cut_into_blocks_without_splitting_a_paragraph() {
        let paragraph = "word ".repeat(100);
        let body: String = (0..20)
            .map(|_| format!("<w:p><w:r><w:t>{paragraph}</w:t></w:r></w:p>"))
            .collect();
        let document = parse(Kind::Docx, &docx(&body)).unwrap();
        assert!(document.blocks.len() > 1);
        assert!(document
            .blocks
            .iter()
            .all(|b| b.text.chars().count() <= 3_600));
        let total: usize = document.blocks.iter().map(|b| b.text.lines().count()).sum();
        assert_eq!(total, 20);
    }

    const SS: &str = "xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"";

    fn xlsx(sheet1: &str) -> Vec<u8> {
        zip_of(&[
            (
                "xl/workbook.xml",
                &format!("<workbook {SS}><sheets><sheet name=\"销售 &amp; 成本\" sheetId=\"1\" r:id=\"rId7\"/><sheet name=\"Empty\" sheetId=\"2\" r:id=\"rId8\"/></sheets></workbook>"),
            ),
            (
                "xl/_rels/workbook.xml.rels",
                "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
                 <Relationship Id=\"rId7\" Type=\"x\" Target=\"worksheets/data.xml\"/>\
                 <Relationship Id=\"rId8\" Type=\"x\" Target=\"/xl/worksheets/other.xml\"/></Relationships>",
            ),
            (
                "xl/sharedStrings.xml",
                &format!("<sst {SS}><si><t>地区</t></si><si><r><t>Rev</t></r><r><t>enue</t></r><rPh><t>ignored</t></rPh></si><si><t>a &amp; b</t></si></sst>"),
            ),
            ("xl/worksheets/data.xml", sheet1),
            ("xl/worksheets/other.xml", &format!("<worksheet {SS}><sheetData/></worksheet>")),
        ])
    }

    #[test]
    fn xlsx_rows_keep_column_positions_types_formulas_and_named_sheets() {
        let sheet = format!(
            "<worksheet {SS}><sheetData>\
             <row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c><c r=\"B1\" t=\"s\"><v>1</v></c></row>\
             <row r=\"2\"><c r=\"A2\" t=\"s\"><v>2</v></c><c r=\"C2\"><v>12.5</v></c><c r=\"D2\" t=\"b\"><v>1</v></c></row>\
             <row r=\"4\"><c r=\"B4\"><f>SUM(B1:B3)</f></c><c r=\"C4\" t=\"inlineStr\"><is><t>inline</t></is></c><c r=\"AA4\"><f>1+1</f><v>2</v></c></row>\
             <row r=\"5\"/></sheetData></worksheet>"
        );
        let document = parse(Kind::Xlsx, &xlsx(&sheet)).unwrap();
        assert_eq!(document.blocks.len(), 2);
        assert_eq!(document.blocks[0].label, "sheet '销售 & 成本'");
        assert_eq!(document.blocks[0].rows, Some(3));
        let lines: Vec<&str> = document.blocks[0].text.lines().collect();
        assert_eq!(lines[0], "1: 地区 | Revenue");
        assert_eq!(lines[1], "2: a & b |  | 12.5 | TRUE");
        assert!(
            lines[2].starts_with("4:  | =SUM(B1:B3) | inline |"),
            "{}",
            lines[2]
        );
        assert!(lines[2].ends_with("| 2"), "{}", lines[2]);
        assert_eq!(document.blocks[1].text, "");
    }

    #[test]
    fn pptx_slides_come_in_numeric_order_with_notes_and_without_slide_numbers() {
        const A: &str = "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"";
        let slide = |title: &str| {
            format!("<p:sld {A}><a:p><a:r><a:t>{title}</a:t></a:r></a:p><a:p><a:fld type=\"slidenum\"><a:t>9</a:t></a:fld></a:p><a:p><a:r><a:t>line</a:t><a:br/><a:t>two</a:t></a:r></a:p></p:sld>")
        };
        let bytes = zip_of(&[
            ("ppt/slides/slide10.xml", &slide("ten")),
            ("ppt/slides/slide2.xml", &slide("two")),
            ("ppt/slides/slide1.xml", &slide("one")),
            (
                "ppt/notesSlides/notesSlide2.xml",
                &format!("<p:notes {A}><a:p><a:r><a:t>say this</a:t></a:r></a:p></p:notes>"),
            ),
        ]);
        let document = parse(Kind::Pptx, &bytes).unwrap();
        let labels: Vec<&str> = document.blocks.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(labels, ["slide 1: one", "slide 2: two", "slide 3: ten"]);
        assert_eq!(
            document.blocks[1].text,
            "two\nline\ntwo\n\nNotes:\nsay this"
        );
        assert!(!document.blocks[0].text.contains('9'));
    }

    /// The smallest PDF that has text: one Helvetica line per page.
    fn pdf(pages: &[&str]) -> Vec<u8> {
        let mut objects: Vec<String> = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            format!(
                "<< /Type /Pages /Count {} /Kids [{}] >>",
                pages.len(),
                (0..pages.len())
                    .map(|i| format!("{} 0 R", 4 + i * 2))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        ];
        for (i, text) in pages.iter().enumerate() {
            let content = if text.is_empty() {
                String::new()
            } else {
                format!("BT /F1 12 Tf 72 720 Td ({text}) Tj ET")
            };
            objects.push(format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
                5 + i * 2
            ));
            objects.push(format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            ));
        }
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, object) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
        }
        let xref = out.len();
        out.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
        );
        for offset in offsets {
            out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        out
    }

    #[test]
    fn pdf_pages_become_blocks_and_a_page_without_text_is_flagged_as_a_scan() {
        let document = parse(Kind::Pdf, &pdf(&["Hello PDF", "", "Third page"])).unwrap();
        assert_eq!(document.blocks.len(), 3);
        assert_eq!(document.blocks[0].text, "Hello PDF");
        assert!(document.blocks[1].scanned);
        assert_eq!(document.blocks[2].label, "page 3");
    }

    #[test]
    fn broken_input_is_an_error_not_a_panic() {
        assert!(parse(Kind::Pdf, b"%PDF-1.4 truncated").is_err());
        assert!(parse(Kind::Pdf, b"").is_err());
        assert!(parse(Kind::Docx, b"not a zip").is_err());
        assert!(parse(Kind::Docx, &zip_of(&[("other.xml", "<a/>")]))
            .unwrap_err()
            .contains("word/document.xml"));
        // Cut-off XML may read as far as it goes or fail; either way, no panic.
        let _ = parse(Kind::Docx, &docx("<w:p><w:r><w:t>broken"));
        assert!(parse(Kind::Pptx, &zip_of(&[("x", "y")]))
            .unwrap_err()
            .contains("no slides"));
    }

    #[test]
    fn file_types_are_decided_by_extension_with_helpful_refusals() {
        assert_eq!(kind_of(Path::new("A.PDF")).unwrap(), Kind::Pdf);
        assert_eq!(kind_of(Path::new("a.xlsm")).unwrap(), Kind::Xlsx);
        assert!(kind_of(Path::new("a.doc"))
            .unwrap_err()
            .contains("save it as .docx"));
        assert!(kind_of(Path::new("a.txt"))
            .unwrap_err()
            .contains("read_file"));
        assert!(kind_of(Path::new("noext")).is_err());
    }

    #[test]
    fn describe_lists_blocks_or_returns_one_and_rejects_bad_indexes() {
        let document = parse(Kind::Pdf, &pdf(&["one", "two"])).unwrap();
        let all = describe(&document, "d.pdf", None).unwrap();
        assert_eq!(all["block_count"], 2);
        assert_eq!(all["blocks"][1]["label"], "page 2");
        assert!(all["text"].as_str().unwrap().contains("[page 2]\ntwo"));
        let one = describe(&document, "d.pdf", Some(1)).unwrap();
        assert_eq!(one["block"]["text"], "two");
        assert!(describe(&document, "d.pdf", Some(2))
            .unwrap_err()
            .contains("0 to 1"));
        assert!(describe(&document, "d.pdf", Some(-1)).is_err());
    }

    #[test]
    fn a_huge_block_is_clipped_and_marked() {
        let long = Document {
            kind: Kind::Docx,
            blocks: vec![Block {
                index: 0,
                label: "part 1".into(),
                text: "x".repeat(MAX_CHARS + 5),
                scanned: false,
                rows: None,
            }],
        };
        let one = describe(&long, "d.docx", Some(0)).unwrap();
        assert_eq!(one["block"]["truncated"], true);
        assert_eq!(
            one["block"]["text"].as_str().unwrap().chars().count(),
            MAX_CHARS
        );
    }

    /// Reads every sample under `DELTA_FIXTURE_DIR` and prints what the model would see. Real
    /// files written by Word, Excel, PowerPoint-compatible tools and PDF producers.
    #[test]
    #[ignore = "needs sample files"]
    fn show_real_files() {
        let dir = std::env::var("DELTA_FIXTURE_DIR").expect("DELTA_FIXTURE_DIR");
        for name in ["zh.docx", "book.xlsx", "deck.pptx", "en.pdf", "zh.pdf"] {
            let document = read_file(&Path::new(&dir).join(name)).unwrap();
            println!(
                "=== {name}\n{}",
                serde_json::to_string_pretty(&describe(&document, name, None).unwrap()).unwrap()
            );
        }
    }
}
