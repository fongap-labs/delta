//! PPTX: text replaced inside the paragraphs of one slide. A slide is named by its number, the
//! same one `read_document` shows ("slide 2"). Speaker notes are not edited.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::{runs, text_edit, Changes};

pub(super) fn edit(bytes: &[u8], edits: &[Value]) -> Result<Changes, String> {
    let mut archive = super::ooxml::open(bytes)?;
    let names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    let slides = super::ooxml::numbered(names.into_iter(), "ppt/slides/slide");
    let mut by_slide: BTreeMap<usize, Vec<runs::TextEdit>> = BTreeMap::new();
    for (index, edit) in edits.iter().enumerate() {
        let number = index + 1;
        let slide = edit
            .get("slide")
            .and_then(Value::as_u64)
            .filter(|slide| *slide >= 1)
            .ok_or_else(|| {
                format!("edit {number}: slide is required (1-based, as read_document shows it)")
            })? as usize;
        if slide > slides.len() {
            return Err(format!(
                "edit {number}: there is no slide {slide}; the presentation has {}",
                slides.len()
            ));
        }
        by_slide
            .entry(slide)
            .or_default()
            .push(text_edit(edit, number)?);
    }
    let mut parts = Vec::new();
    let mut paragraphs = Vec::new();
    let mut counts: BTreeMap<usize, usize> = BTreeMap::new();
    for (slide, slide_edits) in &by_slide {
        let part = &slides[slide - 1].1;
        let xml = super::ooxml::required_part(&mut archive, part)?;
        let (edited, replaced) = runs::replace(&xml, part, slide_edits)?;
        for (edit, count) in slide_edits.iter().zip(&replaced.counts) {
            counts.insert(edit.number, *count);
        }
        paragraphs.push((part.clone(), replaced.changed_paragraphs));
        parts.push((part.clone(), edited));
    }
    let applied = counts
        .into_iter()
        .map(|(edit, count)| json!({"edit": edit, "replaced": count}))
        .collect();
    Ok(Changes {
        parts,
        applied,
        paragraphs,
        rows: Vec::new(),
    })
}
