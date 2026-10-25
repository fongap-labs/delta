//! Replacing text inside the paragraphs of a WordprocessingML or DrawingML part (a DOCX body, a
//! PPTX slide) without touching anything else in it.
//!
//! A paragraph's text is cut into runs (`<t>` elements) wherever the formatting changes, so a
//! phrase can span several runs. A match is replaced inside the run where it starts, which keeps
//! that run's formatting; the matched characters of any later run are removed. The part is
//! rewritten event by event, so every element and attribute outside the changed runs comes through
//! exactly as it was.

use std::collections::{HashMap, HashSet};

use quick_xml::events::{BytesText, Event};
use quick_xml::{Reader, Writer};

use super::super::ooxml::{local, local_end, push_reference, xml_error};

/// Which matches of the text an edit replaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Occurrence {
    /// The only match; more than one is refused.
    Only,
    /// The nth match (1-based), counted in document order.
    Nth(usize),
    /// Every match.
    All,
}

pub(super) struct TextEdit {
    /// The edit's place in the list the caller was given (1-based), for error messages.
    pub number: usize,
    pub find: String,
    pub replace: String,
    pub occurrence: Occurrence,
}

pub(super) struct Replaced {
    /// How many matches each edit replaced, in the order the edits were given.
    pub counts: Vec<usize>,
    /// How many paragraphs an edit touched.
    pub changed_paragraphs: usize,
}

/// One `<t>` element: its text and where its tags sit in the event list.
struct Run {
    text: String,
    first: usize,
    last: usize,
}

/// Replace text in the paragraphs of one XML part. The paragraph and text elements are `p` and `t`
/// in both formats (`w:p`/`w:t` in Word, `a:p`/`a:t` in PowerPoint).
pub(super) fn replace(
    xml: &[u8],
    part: &str,
    edits: &[TextEdit],
) -> Result<(Vec<u8>, Replaced), String> {
    let (events, paragraphs) = scan(xml, part)?;
    let mut texts: Vec<Vec<String>> = paragraphs
        .iter()
        .map(|runs| runs.iter().map(|run| run.text.clone()).collect())
        .collect();
    let mut counts = Vec::with_capacity(edits.len());
    let mut changed: HashSet<usize> = HashSet::new();
    for edit in edits {
        let found = matches_of(&texts, &edit.find);
        let selected = select(&found, edit.occurrence, edit.number)?;
        counts.push(selected.len());
        // Last match first, so the offsets of the earlier ones in a paragraph stay valid.
        for &(paragraph, offset) in selected.iter().rev() {
            splice(
                &mut texts[paragraph],
                offset,
                edit.find.len(),
                &edit.replace,
            );
            changed.insert(paragraph);
        }
    }

    let mut rewrites: HashMap<usize, (usize, &str)> = HashMap::new();
    for (paragraph, runs) in paragraphs.iter().enumerate() {
        for (position, run) in runs.iter().enumerate() {
            let text = &texts[paragraph][position];
            if *text != run.text {
                rewrites.insert(run.first, (run.last, text.as_str()));
            }
        }
    }
    let mut writer = Writer::new(Vec::new());
    let mut index = 0;
    while index < events.len() {
        let Some(&(last, text)) = rewrites.get(&index) else {
            write(&mut writer, events[index].clone(), part)?;
            index += 1;
            continue;
        };
        let Event::Start(start) = &events[index] else {
            return Err(format!(
                "{part} has a text element this edit cannot rewrite"
            ));
        };
        let mut start = start.clone();
        let keeps_spaces = start
            .attributes()
            .flatten()
            .any(|attribute| attribute.key.as_ref() == b"xml:space");
        if !keeps_spaces
            && (text.starts_with(char::is_whitespace) || text.ends_with(char::is_whitespace))
        {
            start.push_attribute(("xml:space", "preserve"));
        }
        write(&mut writer, Event::Start(start), part)?;
        if !text.is_empty() {
            write(&mut writer, Event::Text(BytesText::new(text)), part)?;
        }
        write(&mut writer, events[last].clone(), part)?;
        index = last + 1;
    }
    Ok((
        writer.into_inner(),
        Replaced {
            counts,
            changed_paragraphs: changed.len(),
        },
    ))
}

/// The text of every paragraph of a part, in order. Used to check an edit changed only what it
/// was asked to.
pub(super) fn paragraph_texts(xml: &[u8], part: &str) -> Result<Vec<String>, String> {
    let (_, paragraphs) = scan(xml, part)?;
    Ok(paragraphs
        .iter()
        .map(|runs| runs.iter().map(|run| run.text.as_str()).collect())
        .collect())
}

fn write(writer: &mut Writer<Vec<u8>>, event: Event<'_>, part: &str) -> Result<(), String> {
    writer
        .write_event(event)
        .map_err(|error| format!("could not write {part}: {error}"))
}

/// The part as a list of events, and for each paragraph the runs it holds. A run belongs to the
/// innermost paragraph around it, so a text box inside a paragraph is its own paragraph.
fn scan(xml: &[u8], part: &str) -> Result<(Vec<Event<'static>>, Vec<Vec<Run>>), String> {
    let mut reader = Reader::from_reader(xml);
    let mut events: Vec<Event<'static>> = Vec::new();
    let mut paragraphs: Vec<Vec<Run>> = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    let mut run_start: Option<usize> = None;
    let mut run_text = String::new();
    loop {
        let event = reader
            .read_event()
            .map_err(|error| xml_error(part, error))?
            .into_owned();
        let index = events.len();
        match &event {
            Event::Eof => break,
            Event::Start(start) => match local(start).as_str() {
                "p" => {
                    paragraphs.push(Vec::new());
                    open.push(paragraphs.len() - 1);
                }
                "t" if !open.is_empty() => {
                    run_start = Some(index);
                    run_text.clear();
                }
                _ => {}
            },
            Event::Empty(start) if local(start) == "t" => {
                if let Some(&paragraph) = open.last() {
                    paragraphs[paragraph].push(Run {
                        text: String::new(),
                        first: index,
                        last: index,
                    });
                }
            }
            Event::Text(text) if run_start.is_some() => {
                if let Ok(text) = text.decode() {
                    run_text.push_str(&text);
                }
            }
            Event::GeneralRef(reference) if run_start.is_some() => {
                push_reference(&mut run_text, reference)
            }
            Event::End(end) => match local_end(end).as_str() {
                "t" => {
                    if let (Some(first), Some(&paragraph)) = (run_start.take(), open.last()) {
                        paragraphs[paragraph].push(Run {
                            text: std::mem::take(&mut run_text),
                            first,
                            last: index,
                        });
                    }
                }
                "p" => {
                    open.pop();
                }
                _ => {}
            },
            _ => {}
        }
        events.push(event);
    }
    Ok((events, paragraphs))
}

/// Every match of `find`, as (paragraph, byte offset in the paragraph's text), in document order.
/// A match never crosses a paragraph.
fn matches_of(texts: &[Vec<String>], find: &str) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    for (paragraph, runs) in texts.iter().enumerate() {
        let text: String = runs.concat();
        let mut from = 0;
        while let Some(at) = text[from..].find(find) {
            found.push((paragraph, from + at));
            from += at + find.len();
        }
    }
    found
}

fn select(
    found: &[(usize, usize)],
    occurrence: Occurrence,
    number: usize,
) -> Result<Vec<(usize, usize)>, String> {
    if found.is_empty() {
        return Err(format!("edit {number}: the text to replace was not found"));
    }
    match occurrence {
        Occurrence::Only if found.len() > 1 => Err(format!(
            "edit {number}: the text appears {} times; make it more specific, or set occurrence to a number or \"all\"",
            found.len()
        )),
        Occurrence::Only | Occurrence::All => Ok(found.to_vec()),
        Occurrence::Nth(n) => n
            .checked_sub(1)
            .and_then(|position| found.get(position))
            .map(|&hit| vec![hit])
            .ok_or_else(|| {
                format!(
                    "edit {number}: occurrence {n} does not exist; the text appears {} times",
                    found.len()
                )
            }),
    }
}

/// Replace `length` bytes at `offset` of the runs' joined text. The replacement goes into the run
/// where the match starts, so that run's formatting stays; later runs lose their matched part.
fn splice(runs: &mut [String], offset: usize, length: usize, replacement: &str) {
    let end = offset + length;
    let starts: Vec<usize> = runs
        .iter()
        .scan(0usize, |position, run| {
            let start = *position;
            *position += run.len();
            Some(start)
        })
        .collect();
    let (mut first, mut last) = (None, None);
    for (index, run) in runs.iter().enumerate() {
        if run.is_empty() {
            continue;
        }
        let run_end = starts[index] + run.len();
        if first.is_none() && offset < run_end {
            first = Some(index);
        }
        if first.is_some() && end <= run_end {
            last = Some(index);
            break;
        }
    }
    let (Some(first), Some(last)) = (first, last) else {
        return;
    };
    let prefix = runs[first][..offset - starts[first]].to_string();
    let suffix = runs[last][end - starts[last]..].to_string();
    if first == last {
        runs[first] = format!("{prefix}{replacement}{suffix}");
        return;
    }
    runs[first] = format!("{prefix}{replacement}");
    for run in &mut runs[first + 1..last] {
        run.clear();
    }
    runs[last] = suffix;
}
