//! DOCX: text replaced inside the paragraphs of the document body (`word/document.xml`),
//! tables included. Headers, footers, footnotes and comments are not edited.

use serde_json::{json, Value};

use super::{runs, text_edit, Changes};

const BODY: &str = "word/document.xml";

pub(super) fn edit(bytes: &[u8], edits: &[Value]) -> Result<Changes, String> {
    let parsed = edits
        .iter()
        .enumerate()
        .map(|(index, edit)| text_edit(edit, index + 1))
        .collect::<Result<Vec<_>, _>>()?;
    let mut archive = super::ooxml::open(bytes)?;
    let xml = super::ooxml::required_part(&mut archive, BODY)?;
    let (edited, replaced) = runs::replace(&xml, BODY, &parsed)?;
    let applied = parsed
        .iter()
        .zip(&replaced.counts)
        .map(|(edit, count)| json!({"edit": edit.number, "replaced": count}))
        .collect();
    Ok(Changes {
        parts: vec![(BODY.to_owned(), edited)],
        applied,
        paragraphs: vec![(BODY.to_owned(), replaced.changed_paragraphs)],
        rows: Vec::new(),
    })
}
