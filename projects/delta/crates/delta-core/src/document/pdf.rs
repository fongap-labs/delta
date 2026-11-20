//! PDF text, one block per page.

use lopdf::Document;

/// Text of every page. A page with no extractable text (a scan, or text drawn as outlines) comes
/// back empty and is flagged by the caller; it is never silently skipped.
pub fn read_pdf(bytes: &[u8]) -> Result<Vec<String>, String> {
    let document = Document::load_mem(bytes)
        .map_err(|error| format!("the file is not a readable PDF: {error}"))?;
    if document.is_encrypted() {
        return Err("the PDF is password-protected; remove the password and try again".to_owned());
    }
    let pages: Vec<u32> = document.get_pages().keys().copied().collect();
    if pages.is_empty() {
        return Err("the PDF has no pages".to_owned());
    }
    let mut out = Vec::with_capacity(pages.len());
    for number in pages {
        // A malformed page must cost that page, not the whole read (or the process).
        let text = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            document.extract_text(&[number])
        }))
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
        let cleaned: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        out.push(cleaned.join("\n"));
    }
    Ok(out)
}
