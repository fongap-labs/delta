//! Turning one file into searchable chunks, each with a locator that says where in the file it
//! came from. The locators use the vocabulary of `source_citation` (`lines`, `page`, `sheet`, `row`,
//! `custom`), so a hit can be cited without translation.

use serde_json::{json, Value};

use crate::document::{self, Kind};
use crate::web::html::html_to_text;

/// Characters in one chunk, and how far a long paragraph's pieces overlap.
const CHUNK_CHARS: usize = 800;
const OVERLAP_CHARS: usize = 100;
/// Chunks one file may make; a file with more is cut and marked.
pub(super) const MAX_CHUNKS_PER_FILE: usize = 4000;
/// The largest file the index reads.
pub(super) const MAX_FILE_BYTES: u64 = 25 * 1024 * 1024;

pub(super) struct Chunk {
    pub locator: Value,
    pub text: String,
}

pub(super) struct Extracted {
    pub chunks: Vec<Chunk>,
    /// The file made more chunks than one file may, so the rest is not searchable.
    pub is_truncated: bool,
}

/// The kind of file an extension names, or `None` when the index does not read it.
pub(super) fn kind_of(extension: &str) -> Option<&'static str> {
    match extension {
        "txt" | "md" | "markdown" | "csv" | "tsv" | "json" | "yaml" | "yml" | "rst" | "log" => {
            Some("text")
        }
        "html" | "htm" => Some("html"),
        "pdf" => Some("pdf"),
        "docx" => Some("docx"),
        "xlsx" | "xlsm" => Some("xlsx"),
        "pptx" => Some("pptx"),
        _ => None,
    }
}

/// Read `bytes` as the given kind and cut it into chunks.
pub(super) fn extract(kind: &str, bytes: &[u8]) -> Result<Extracted, String> {
    let mut chunks = Vec::new();
    match kind {
        "text" => push_lines(
            &mut chunks,
            &decode(bytes),
            |start, end| json!({"kind": "lines", "start": start, "end": end}),
        ),
        "html" => push_lines(
            &mut chunks,
            &html_to_text(&decode(bytes)),
            |start, end| json!({"kind": "lines", "start": start, "end": end}),
        ),
        "pdf" | "docx" | "xlsx" | "pptx" => {
            let document_kind = match kind {
                "pdf" => Kind::Pdf,
                "docx" => Kind::Docx,
                "xlsx" => Kind::Xlsx,
                _ => Kind::Pptx,
            };
            let document = document::parse(document_kind, bytes)?;
            for block in document.blocks {
                if block.scanned {
                    continue;
                }
                match document_kind {
                    Kind::Pdf => {
                        let page = block.index + 1;
                        push_text(
                            &mut chunks,
                            &block.text,
                            &json!({"kind": "page", "page": page}),
                        );
                    }
                    Kind::Docx => push_text(
                        &mut chunks,
                        &block.text,
                        &json!({"kind": "custom", "descriptor": {"part": block.index + 1}}),
                    ),
                    Kind::Pptx => push_text(
                        &mut chunks,
                        &block.text,
                        &json!({"kind": "custom", "descriptor": {"slide": block.index + 1}}),
                    ),
                    Kind::Xlsx => {
                        let sheet = block
                            .label
                            .strip_prefix("sheet '")
                            .and_then(|rest| rest.strip_suffix('\''))
                            .unwrap_or(&block.label)
                            .to_owned();
                        push_rows(&mut chunks, &block.text, &sheet);
                    }
                }
            }
        }
        other => return Err(format!("unsupported kind: {other}")),
    }
    let is_truncated = chunks.len() > MAX_CHUNKS_PER_FILE;
    chunks.truncate(MAX_CHUNKS_PER_FILE);
    Ok(Extracted {
        chunks,
        is_truncated,
    })
}

/// UTF-8 when the bytes are valid UTF-8, otherwise GBK, which is what most older Chinese text
/// files are. A byte order mark is dropped.
fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => encoding_rs::GBK.decode(bytes).0.into_owned(),
    }
}

/// Chunk line-oriented text, remembering the 1-based line numbers each chunk covers.
fn push_lines(chunks: &mut Vec<Chunk>, text: &str, locator: impl Fn(usize, usize) -> Value) {
    let mut current = String::new();
    let mut start = 0usize;
    let mut last = 0usize;
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let line = line.trim_end();
        if line.trim().is_empty() {
            // A blank line ends a paragraph once the chunk is big enough to stand alone.
            if current.chars().count() >= CHUNK_CHARS / 2 {
                flush(chunks, &mut current, start, last, &locator);
            }
            continue;
        }
        if current.is_empty() {
            start = number;
        }
        // A single very long line (minified text, a one-paragraph file) is cut in pieces.
        if line.chars().count() > CHUNK_CHARS {
            flush(chunks, &mut current, start, last, &locator);
            for piece in split_long(line) {
                chunks.push(Chunk {
                    locator: locator(number, number),
                    text: piece,
                });
            }
            continue;
        }
        if !current.is_empty() && current.chars().count() + line.chars().count() + 1 > CHUNK_CHARS {
            flush(chunks, &mut current, start, last, &locator);
            start = number;
        }
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
        last = number;
    }
    flush(chunks, &mut current, start, last, &locator);
}

fn flush(
    chunks: &mut Vec<Chunk>,
    current: &mut String,
    start: usize,
    end: usize,
    locator: &impl Fn(usize, usize) -> Value,
) {
    if current.trim().is_empty() {
        current.clear();
        return;
    }
    chunks.push(Chunk {
        locator: locator(start, end.max(start)),
        text: std::mem::take(current),
    });
}

/// Pieces of at most `CHUNK_CHARS` characters, each overlapping the one before it.
fn split_long(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let step = CHUNK_CHARS - OVERLAP_CHARS;
    let mut pieces = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        let end = (at + CHUNK_CHARS).min(chars.len());
        pieces.push(chars[at..end].iter().collect());
        if end == chars.len() {
            break;
        }
        at += step;
    }
    pieces
}

/// Chunk the text of one document block (a page, a slide, a part), all with the same locator.
fn push_text(chunks: &mut Vec<Chunk>, text: &str, locator: &Value) {
    let mut local = Vec::new();
    push_lines(&mut local, text, |_, _| locator.clone());
    chunks.extend(local);
}

/// Chunk a sheet's `row: cell | cell` lines, with the first and last row of each chunk.
fn push_rows(chunks: &mut Vec<Chunk>, text: &str, sheet: &str) {
    let row_of = |line: &str| -> Option<u64> {
        line.split_once(':')
            .and_then(|(number, _)| number.trim().parse().ok())
    };
    let mut current = String::new();
    let mut first: Option<u64> = None;
    let mut last: Option<u64> = None;
    let finish = |chunks: &mut Vec<Chunk>,
                  current: &mut String,
                  first: &mut Option<u64>,
                  last: &mut Option<u64>| {
        if current.trim().is_empty() {
            current.clear();
            return;
        }
        let locator = match (*first, *last) {
            (Some(a), Some(b)) => {
                json!({"kind": "row", "sheet": sheet, "row_start": a, "row_end": b})
            }
            _ => json!({"kind": "sheet", "sheet": sheet}),
        };
        chunks.push(Chunk {
            locator,
            text: std::mem::take(current),
        });
        *first = None;
        *last = None;
    };
    for line in text.lines() {
        if !current.is_empty() && current.chars().count() + line.chars().count() + 1 > CHUNK_CHARS {
            finish(chunks, &mut current, &mut first, &mut last);
        }
        // A row longer than a chunk is kept whole: cutting a row would split a record.
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
        if let Some(row) = row_of(line) {
            first.get_or_insert(row);
            last = Some(row);
        }
    }
    finish(chunks, &mut current, &mut first, &mut last);
}
