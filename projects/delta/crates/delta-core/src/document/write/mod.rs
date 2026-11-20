//! Writing the three Office formats from structured content.
//!
//! Each writer builds a complete OOXML package: the minimum parts Word, Excel and PowerPoint
//! (and LibreOffice) need, styled with sensible defaults. They do layout design only as far as
//! "headings, lists, tables, a header row, bullets": the goal is a correct, readable file, not a
//! designed one (blueprint 2.1).

mod docx;
mod pptx;
mod xlsx;

use std::io::Write;

use serde_json::Value;

use super::Kind;

/// Largest text value accepted for one paragraph, cell or bullet.
const MAX_TEXT_CHARS: usize = 100_000;

pub(crate) fn write(kind: Kind, arguments: &Value) -> Result<Vec<u8>, String> {
    match kind {
        Kind::Docx => docx::build(arguments),
        Kind::Xlsx => xlsx::build(arguments),
        Kind::Pptx => pptx::build(arguments),
        Kind::Pdf => {
            Err("create_document cannot make PDFs; create a DOCX and export it to PDF".to_owned())
        }
    }
}

/// `text` without the control characters XML 1.0 forbids (tab, newline and carriage return stay).
pub(crate) fn strip_xml_controls(text: &str) -> String {
    text.chars()
        .filter(|&c| {
            matches!(c, '\t' | '\n' | '\r')
                || ((c as u32) >= 0x20 && c != '\u{FFFE}' && c != '\u{FFFF}')
        })
        .collect()
}

/// Escape text for an XML text node or attribute.
pub(super) fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in strip_xml_controls(text).chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

/// A string argument, trimmed of nothing, refused when too long.
pub(super) fn text_of(value: &Value, what: &str) -> Result<String, String> {
    let text = match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        _ => return Err(format!("{what} must be text")),
    };
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err(format!(
            "{what} is too long (over {MAX_TEXT_CHARS} characters)"
        ));
    }
    Ok(text)
}

/// The XML declaration every part starts with.
pub(super) const XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";

/// Zip `(part name, content)` pairs into a package.
pub(super) fn package(parts: Vec<(String, String)>) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut out));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, content) in parts {
            writer
                .start_file(name, options)
                .map_err(|error| format!("could not build the file: {error}"))?;
            writer
                .write_all(content.as_bytes())
                .map_err(|error| format!("could not build the file: {error}"))?;
        }
        writer
            .finish()
            .map_err(|error| format!("could not build the file: {error}"))?;
    }
    Ok(out)
}

pub(super) fn array<'a>(arguments: &'a Value, key: &str) -> Result<&'a Vec<Value>, String> {
    arguments
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{key} is required and must be a list"))
}
