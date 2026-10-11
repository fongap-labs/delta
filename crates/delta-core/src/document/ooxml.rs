//! Readers for the three Office formats: DOCX, XLSX and PPTX are ZIP files of XML parts. They
//! pull out text and structure, not layout or formatting (blueprint 2.1: not an Office suite).

use std::collections::HashMap;
use std::io::{Cursor, Read};

use quick_xml::events::{BytesEnd, BytesRef, BytesStart, Event};
use quick_xml::Reader;
use zip::ZipArchive;

/// Largest XML part read, after decompression. Bounds a ZIP bomb.
const MAX_PART_BYTES: u64 = 64 * 1024 * 1024;

type Archive<'a> = ZipArchive<Cursor<&'a [u8]>>;

fn open(bytes: &[u8]) -> Result<Archive<'_>, String> {
    ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| format!("the file is not a valid Office document: {error}"))
}

fn part(archive: &mut Archive<'_>, name: &str) -> Result<Option<Vec<u8>>, String> {
    let file = match archive.by_name(name) {
        Ok(file) => file,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(error) => return Err(format!("could not read {name}: {error}")),
    };
    if file.size() > MAX_PART_BYTES {
        return Err(format!("{name} is too large to read"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_PART_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("could not read {name}: {error}"))?;
    Ok(Some(bytes))
}

fn required_part(archive: &mut Archive<'_>, name: &str) -> Result<Vec<u8>, String> {
    part(archive, name)?
        .ok_or_else(|| format!("the file has no {name}; it is not a complete document"))
}

fn xml_reader(bytes: &[u8]) -> Reader<&[u8]> {
    Reader::from_reader(bytes)
}

/// Local name of an element: `w:t` and `t` are both `t`.
fn local(start: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(start.local_name().as_ref()).into_owned()
}

fn local_end(end: &BytesEnd<'_>) -> String {
    String::from_utf8_lossy(end.local_name().as_ref()).into_owned()
}

/// Value of the attribute whose local name is `name` (`w:val`, `r:id`, `val` all match `val`/`id`).
fn attr(start: &BytesStart<'_>, name: &str) -> Option<String> {
    start.attributes().flatten().find_map(|attribute| {
        (attribute.key.local_name().as_ref() == name.as_bytes())
            .then(|| {
                attribute
                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                    .ok()
                    .map(|value| value.into_owned())
            })
            .flatten()
    })
}

/// `&amp;`, `&#x4e2d;` and friends, which quick-xml hands over as separate events.
fn push_reference(out: &mut String, reference: &BytesRef<'_>) {
    if let Ok(Some(c)) = reference.resolve_char_ref() {
        out.push(c);
        return;
    }
    match reference.decode().as_deref() {
        Ok("amp") => out.push('&'),
        Ok("lt") => out.push('<'),
        Ok("gt") => out.push('>'),
        Ok("quot") => out.push('"'),
        Ok("apos") => out.push('\''),
        _ => {}
    }
}

fn xml_error(name: &str, error: quick_xml::Error) -> String {
    format!("{name} is not valid XML: {error}")
}

/// Join lines into blocks of roughly `target` characters, never splitting a line.
pub fn pack_blocks(lines: Vec<String>, target: usize) -> Vec<Vec<String>> {
    let mut blocks: Vec<Vec<String>> = Vec::new();
    let mut size = 0;
    for line in lines {
        if blocks.is_empty() || size + line.chars().count() > target && size > 0 {
            blocks.push(Vec::new());
            size = 0;
        }
        size += line.chars().count() + 1;
        blocks
            .last_mut()
            .expect("a block was just pushed")
            .push(line);
    }
    blocks
}

// -- DOCX ------------------------------------------------------------------------------------

/// Paragraph and table text of a DOCX in document order. Headings keep a `#` prefix, a table row is
/// one line with cells joined by ` | `.
pub fn read_docx(bytes: &[u8]) -> Result<Vec<String>, String> {
    let mut archive = open(bytes)?;
    let xml = required_part(&mut archive, "word/document.xml")?;
    let mut reader = xml_reader(&xml);
    let mut lines = Vec::new();
    let mut paragraph = String::new();
    let mut heading = 0usize;
    let mut in_text = false;
    let mut table_depth = 0usize;
    let mut cell = String::new();
    let mut row: Vec<String> = Vec::new();
    loop {
        match reader
            .read_event()
            .map_err(|e| xml_error("word/document.xml", e))?
        {
            Event::Start(e) | Event::Empty(e) => match local(&e).as_str() {
                "p" => {
                    paragraph.clear();
                    heading = 0;
                }
                "pStyle" => {
                    if let Some(style) = attr(&e, "val") {
                        heading = if style == "Title" {
                            1
                        } else {
                            style
                                .strip_prefix("Heading")
                                .and_then(|level| level.trim().parse::<usize>().ok())
                                .unwrap_or(0)
                        };
                    }
                }
                "t" => in_text = true,
                "tab" => paragraph.push('\t'),
                "br" | "cr" => paragraph.push('\n'),
                "tbl" => table_depth += 1,
                "tr" => row.clear(),
                "tc" => cell.clear(),
                _ => {}
            },
            Event::Text(text) if in_text => {
                if let Ok(text) = text.decode() {
                    paragraph.push_str(&text);
                }
            }
            Event::GeneralRef(reference) if in_text => push_reference(&mut paragraph, &reference),
            Event::End(e) => match local_end(&e).as_str() {
                "t" => in_text = false,
                "p" => {
                    let text = paragraph.trim();
                    if table_depth > 0 {
                        if !text.is_empty() {
                            if !cell.is_empty() {
                                cell.push(' ');
                            }
                            cell.push_str(text);
                        }
                    } else if !text.is_empty() {
                        let prefix = if heading > 0 {
                            "#".repeat(heading.min(6)) + " "
                        } else {
                            String::new()
                        };
                        lines.push(format!("{prefix}{text}"));
                    }
                    paragraph.clear();
                }
                "tc" => row.push(cell.trim().to_owned()),
                "tr" => {
                    if row.iter().any(|cell| !cell.is_empty()) {
                        lines.push(row.join(" | "));
                    }
                    row.clear();
                }
                "tbl" => table_depth = table_depth.saturating_sub(1),
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(lines)
}

// -- XLSX ------------------------------------------------------------------------------------

/// One worksheet: its name, how many rows have content, and one line per row (`12: a | b | c`).
pub struct Sheet {
    pub name: String,
    pub row_count: usize,
    pub lines: Vec<String>,
}

/// Column index (A = 0) from a cell reference such as `AB12`.
fn column_of(reference: &str) -> usize {
    reference
        .chars()
        .take_while(char::is_ascii_alphabetic)
        .fold(0usize, |acc, c| {
            acc * 26 + (c.to_ascii_uppercase() as usize - 'A' as usize + 1)
        })
        .saturating_sub(1)
}

fn shared_strings(archive: &mut Archive<'_>) -> Result<Vec<String>, String> {
    let Some(xml) = part(archive, "xl/sharedStrings.xml")? else {
        return Ok(Vec::new());
    };
    let mut reader = xml_reader(&xml);
    let mut strings = Vec::new();
    let mut current = String::new();
    let (mut in_text, mut phonetic) = (false, 0usize);
    loop {
        match reader
            .read_event()
            .map_err(|e| xml_error("xl/sharedStrings.xml", e))?
        {
            Event::Start(e) => match local(&e).as_str() {
                "si" => current.clear(),
                // Furigana and similar annotations are not part of the cell's text.
                "rPh" => phonetic += 1,
                "t" => in_text = phonetic == 0,
                _ => {}
            },
            Event::Text(text) if in_text => {
                if let Ok(text) = text.decode() {
                    current.push_str(&text);
                }
            }
            Event::GeneralRef(reference) if in_text => push_reference(&mut current, &reference),
            Event::End(e) => match local_end(&e).as_str() {
                "t" => in_text = false,
                "rPh" => phonetic = phonetic.saturating_sub(1),
                "si" => strings.push(std::mem::take(&mut current)),
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(strings)
}

/// Sheet names with the part each one lives in, in workbook order.
fn sheet_parts(archive: &mut Archive<'_>) -> Result<Vec<(String, String)>, String> {
    let workbook = required_part(archive, "xl/workbook.xml")?;
    let mut reader = xml_reader(&workbook);
    let mut sheets: Vec<(String, String)> = Vec::new();
    loop {
        match reader
            .read_event()
            .map_err(|e| xml_error("xl/workbook.xml", e))?
        {
            Event::Start(e) | Event::Empty(e) if local(&e) == "sheet" => {
                if let (Some(name), Some(id)) = (attr(&e, "name"), attr(&e, "id")) {
                    sheets.push((name, id));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let mut targets: HashMap<String, String> = HashMap::new();
    if let Some(rels) = part(archive, "xl/_rels/workbook.xml.rels")? {
        let mut reader = xml_reader(&rels);
        loop {
            match reader
                .read_event()
                .map_err(|e| xml_error("xl/_rels/workbook.xml.rels", e))?
            {
                Event::Start(e) | Event::Empty(e) if local(&e) == "Relationship" => {
                    if let (Some(id), Some(target)) = (attr(&e, "Id"), attr(&e, "Target")) {
                        targets.insert(id, target);
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
    }
    Ok(sheets
        .into_iter()
        .enumerate()
        .map(|(index, (name, id))| {
            let path = match targets.get(&id) {
                Some(target) => match target.strip_prefix('/') {
                    Some(absolute) => absolute.to_owned(),
                    None => format!("xl/{target}"),
                },
                None => format!("xl/worksheets/sheet{}.xml", index + 1),
            };
            (name, path)
        })
        .collect())
}

fn read_sheet(xml: &[u8], name: &str, strings: &[String]) -> Result<Sheet, String> {
    let mut reader = xml_reader(xml);
    let mut lines = Vec::new();
    let mut row_number = 0usize;
    let mut cells: Vec<(usize, String)> = Vec::new();
    let (mut kind, mut reference, mut value) = (String::new(), String::new(), String::new());
    let mut formula = String::new();
    let (mut in_value, mut in_inline, mut in_formula) = (false, false, false);
    loop {
        match reader.read_event().map_err(|e| xml_error(name, e))? {
            Event::Start(e) | Event::Empty(e) => match local(&e).as_str() {
                "row" => {
                    row_number = attr(&e, "r")
                        .and_then(|r| r.parse().ok())
                        .unwrap_or(row_number + 1);
                    cells.clear();
                }
                "c" => {
                    kind = attr(&e, "t").unwrap_or_default();
                    reference = attr(&e, "r").unwrap_or_default();
                    value.clear();
                    formula.clear();
                }
                "v" => in_value = true,
                "f" => in_formula = true,
                "t" if in_inline => in_value = true,
                "is" => in_inline = true,
                _ => {}
            },
            Event::Text(text) if in_value || in_formula => {
                if let Ok(text) = text.decode() {
                    if in_value {
                        value.push_str(&text);
                    } else {
                        formula.push_str(&text);
                    }
                }
            }
            Event::GeneralRef(reference) if in_value => push_reference(&mut value, &reference),
            Event::GeneralRef(reference) if in_formula => push_reference(&mut formula, &reference),
            Event::End(e) => match local_end(&e).as_str() {
                "v" | "t" => in_value = false,
                "f" => in_formula = false,
                "is" => in_inline = false,
                "c" => {
                    let shown = match kind.as_str() {
                        "s" => value
                            .trim()
                            .parse::<usize>()
                            .ok()
                            .and_then(|i| strings.get(i))
                            .cloned()
                            .unwrap_or_default(),
                        "b" => {
                            if value.trim() == "1" {
                                "TRUE".to_owned()
                            } else {
                                "FALSE".to_owned()
                            }
                        }
                        _ => value.clone(),
                    };
                    // A formula nobody has calculated yet (the file was written by a program) has
                    // no value; show the formula rather than an empty cell.
                    let shown = if shown.is_empty() && !formula.trim().is_empty() {
                        format!("={}", formula.trim())
                    } else {
                        shown
                    };
                    if !shown.is_empty() {
                        let column = if reference.is_empty() {
                            cells.len()
                        } else {
                            column_of(&reference)
                        };
                        cells.push((column, shown.replace('\n', " ")));
                    }
                }
                "row" if !cells.is_empty() => {
                    let width = cells
                        .iter()
                        .map(|(column, _)| column + 1)
                        .max()
                        .unwrap_or(0);
                    let mut line = vec![String::new(); width];
                    for (column, text) in cells.drain(..) {
                        line[column] = text;
                    }
                    lines.push(format!("{row_number}: {}", line.join(" | ")));
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(Sheet {
        name: name.to_owned(),
        row_count: lines.len(),
        lines,
    })
}

pub fn read_xlsx(bytes: &[u8]) -> Result<Vec<Sheet>, String> {
    let mut archive = open(bytes)?;
    let strings = shared_strings(&mut archive)?;
    let mut sheets = Vec::new();
    for (name, path) in sheet_parts(&mut archive)? {
        let xml = required_part(&mut archive, &path)?;
        sheets.push(read_sheet(&xml, &name, &strings)?);
    }
    Ok(sheets)
}

// -- PPTX ------------------------------------------------------------------------------------

/// One slide: its paragraphs of text, and the speaker notes if it has any.
pub struct Slide {
    pub lines: Vec<String>,
    pub notes: Vec<String>,
}

fn drawing_text(xml: &[u8], name: &str) -> Result<Vec<String>, String> {
    let mut reader = xml_reader(xml);
    let mut lines = Vec::new();
    let mut paragraph = String::new();
    let (mut in_text, mut field) = (false, 0usize);
    loop {
        match reader.read_event().map_err(|e| xml_error(name, e))? {
            Event::Start(e) => match local(&e).as_str() {
                "p" => paragraph.clear(),
                // Slide numbers and dates are fields, not content.
                "fld" => field += 1,
                "t" => in_text = field == 0,
                _ => {}
            },
            Event::Empty(e) if matches!(local(&e).as_str(), "br") => paragraph.push('\n'),
            Event::Text(text) if in_text => {
                if let Ok(text) = text.decode() {
                    paragraph.push_str(&text);
                }
            }
            Event::GeneralRef(reference) if in_text => push_reference(&mut paragraph, &reference),
            Event::End(e) => match local_end(&e).as_str() {
                "t" => in_text = false,
                "fld" => field = field.saturating_sub(1),
                "p" => {
                    let text = paragraph.trim();
                    if !text.is_empty() {
                        lines.push(text.to_owned());
                    }
                    paragraph.clear();
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(lines)
}

fn numbered(names: impl Iterator<Item = String>, prefix: &str) -> Vec<(usize, String)> {
    let mut found: Vec<(usize, String)> = names
        .filter_map(|name| {
            let number = name
                .strip_prefix(prefix)?
                .strip_suffix(".xml")?
                .parse::<usize>()
                .ok()?;
            Some((number, name))
        })
        .collect();
    found.sort();
    found
}

pub fn read_pptx(bytes: &[u8]) -> Result<Vec<Slide>, String> {
    let mut archive = open(bytes)?;
    let names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    let slides = numbered(names.iter().cloned(), "ppt/slides/slide");
    if slides.is_empty() {
        return Err("the file has no slides; it is not a complete presentation".to_owned());
    }
    let notes: HashMap<usize, String> =
        numbered(names.iter().cloned(), "ppt/notesSlides/notesSlide")
            .into_iter()
            .collect();
    let mut out = Vec::new();
    for (number, name) in slides {
        let xml = required_part(&mut archive, &name)?;
        let lines = drawing_text(&xml, &name)?;
        let notes = match notes.get(&number) {
            Some(path) => match part(&mut archive, path)? {
                Some(xml) => drawing_text(&xml, path)?,
                None => Vec::new(),
            },
            None => Vec::new(),
        };
        out.push(Slide { lines, notes });
    }
    Ok(out)
}
