//! Rendering a report from a template of bindings, and checking a report against its manifest
//! (ADR-0055 points 2 and 3).
//!
//! A template is Markdown with `{{result:<path>#<key>|<format>}}` where a computed number goes.
//! Rendering reads each result file (the JSON `values` object `statistics_record` writes), formats
//! the number with the declared format, and writes the report together with a manifest that says,
//! for every binding, which file (and SHA-256) and key it came from, the raw value, the format, the
//! rendered text and where in the report it sits. Checking recomputes all of that from the files as
//! they are now, so a changed result file, a hand-edited number or a moved binding is found. A
//! formal report may not contain any other number except those its criteria allow.

use std::collections::HashMap;

use regex::{Regex, RegexBuilder};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::{parse_binding, parse_format, render, Binding};

const OPEN: &str = "{{result:";
const CLOSE: &str = "}}";
pub const MANIFEST_FORMAT: &str = "delta.binding-manifest";
pub const MANIFEST_VERSION: u64 = 1;
/// The most bytes of a template, a report, a manifest or a result file read.
pub const MAX_BYTES: u64 = 5 * 1024 * 1024;
const MAX_ALLOWED_PATTERNS: usize = 20;
const MAX_PATTERN_CHARS: usize = 200;
/// The unbound numbers a check lists; the count is always complete.
const MAX_LISTED_UNBOUND: usize = 20;

/// Reads a file by the path a template or manifest names it with.
pub type Reader<'a> = &'a dyn Fn(&str) -> Result<Vec<u8>, String>;

/// One binding as rendered, as it is written in the manifest.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedBinding {
    pub artifact: String,
    pub sha256: String,
    pub key: String,
    pub format: String,
    pub raw: f64,
    pub rendered: String,
    /// Characters from the start of the report to the start of the rendered text.
    pub offset: usize,
    /// The heading the binding sits under, when there is one.
    pub section: Option<String>,
}

impl RenderedBinding {
    fn to_json(&self) -> Value {
        json!({
            "artifact": self.artifact, "sha256": self.sha256, "key": self.key,
            "format": self.format, "raw": self.raw, "rendered": self.rendered,
            "offset": self.offset, "section": self.section,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    pub text: String,
    pub bindings: Vec<RenderedBinding>,
}

impl Rendered {
    /// The manifest, for a report written to `report_path` from `template_path`.
    pub fn manifest(&self, report_path: &str, template_path: &str) -> Value {
        json!({
            "format": MANIFEST_FORMAT, "version": MANIFEST_VERSION,
            "report": report_path, "template": template_path,
            "bindings": self.bindings.iter().map(RenderedBinding::to_json).collect::<Vec<_>>(),
        })
    }
}

/// The number under `key` in a result file, and the file's SHA-256.
fn load_value(
    cache: &mut HashMap<String, Result<(String, Value), String>>,
    read: Reader<'_>,
    artifact: &str,
    key: &str,
) -> Result<(String, f64), String> {
    let loaded = cache.entry(artifact.to_owned()).or_insert_with(|| {
        let bytes = read(artifact)?;
        let sha256 = format!("{:x}", Sha256::digest(&bytes));
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| format!("{artifact} is not a JSON result file"))?;
        Ok((sha256, value))
    });
    let (sha256, value) = loaded.as_ref().map_err(|error| error.clone())?;
    let number = value
        .get("values")
        .and_then(|values| values.get(key))
        .and_then(|entry| entry.get("value"))
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("{artifact} has no value under {key}"))?;
    if !number.is_finite() {
        return Err(format!("{artifact}: {key} is not a finite number"));
    }
    Ok((sha256.clone(), number))
}

/// The heading text of the closest heading before `at` in `text`.
fn section_at(text: &str, at: usize) -> Option<String> {
    text[..at]
        .lines()
        .rev()
        .find(|line| line.trim_start().starts_with('#'))
        .map(|line| line.trim_start().trim_start_matches('#').trim().to_owned())
        .filter(|heading| !heading.is_empty())
}

/// Render `template`. Every problem found is returned together, not only the first.
pub fn render_template(template: &str, read: Reader<'_>) -> Result<Rendered, Vec<String>> {
    let mut cache = HashMap::new();
    let mut text = String::with_capacity(template.len());
    let mut bindings = Vec::new();
    let mut problems = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find(OPEN) {
        text.push_str(&rest[..start]);
        let after = &rest[start..];
        let Some(end) = after.find(CLOSE) else {
            problems.push("a binding is not closed with }}".to_owned());
            rest = "";
            break;
        };
        let source = &after[..end + CLOSE.len()];
        rest = &after[end + CLOSE.len()..];
        let binding: Binding = match parse_binding(source) {
            Ok(binding) => binding,
            Err(error) => {
                problems.push(format!("{source}: {error}"));
                continue;
            }
        };
        let format_text = source
            .rsplit_once('|')
            .map(|(_, format)| format.trim_end_matches(CLOSE).trim().to_owned())
            .unwrap_or_default();
        match load_value(&mut cache, read, &binding.path, &binding.key).and_then(|(sha256, raw)| {
            render(raw, binding.format).map(|rendered| (sha256, raw, rendered))
        }) {
            Ok((sha256, raw, rendered)) => {
                let offset = text.chars().count();
                let section = section_at(&text, text.len());
                text.push_str(&rendered);
                bindings.push(RenderedBinding {
                    artifact: binding.path,
                    sha256,
                    key: binding.key,
                    format: format_text,
                    raw,
                    rendered,
                    offset,
                    section,
                });
            }
            Err(error) => problems.push(format!("{source}: {error}")),
        }
    }
    text.push_str(rest);
    if problems.is_empty() {
        Ok(Rendered { text, bindings })
    } else {
        Err(problems)
    }
}

/// A number in a report that no binding accounts for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnboundNumber {
    pub text: String,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CheckReport {
    pub problems: Vec<String>,
    pub bindings: usize,
    pub artifacts: usize,
    /// Numbers outside any binding and outside every allowed pattern. Filled for a formal report.
    pub unbound: Vec<UnboundNumber>,
    pub is_formal: bool,
}

impl CheckReport {
    pub fn passed(&self) -> bool {
        self.problems.is_empty() && self.unbound.is_empty()
    }

    pub fn to_json(&self) -> Value {
        json!({
            "passed": self.passed(), "formal": self.is_formal,
            "bindings": self.bindings, "artifacts": self.artifacts,
            "problems": self.problems,
            "unbound_count": self.unbound.len(),
            "unbound_numbers": self.unbound.iter().take(MAX_LISTED_UNBOUND)
                .map(|number| json!({"text": number.text, "line": number.line})).collect::<Vec<_>>(),
            "note": if self.is_formal { Value::Null } else {
                json!("Not a formal report: numbers outside the bindings were not checked.")
            },
        })
    }
}

/// Patterns that are always allowed: the numbers of headings and of numbered lists. Everything
/// else (dates, conditions such as 25 °C / 60 % RH, section references) a criterion must name.
const DEFAULT_ALLOWED: &[&str] = &[
    r"(?m)^[ \t]{0,3}#{1,6}[ \t]+\d+(?:\.\d+)*[.)]?",
    r"(?m)^[ \t]*\d+[.)][ \t]",
];

fn compile(pattern: &str) -> Result<Regex, String> {
    if pattern.chars().count() > MAX_PATTERN_CHARS {
        return Err(format!(
            "an allowed pattern is longer than {MAX_PATTERN_CHARS} characters"
        ));
    }
    RegexBuilder::new(pattern)
        .size_limit(1 << 20)
        .build()
        .map_err(|error| format!("allowed pattern {pattern:?} is not valid: {error}"))
}

/// Check `report` against `manifest`. `read` returns a result file's bytes. A formal report also
/// may not hold a number outside its bindings except where one of `allowed` (regular expressions)
/// or a default pattern covers it.
pub fn check_report(
    report: &str,
    manifest: &Value,
    read: Reader<'_>,
    is_formal: bool,
    allowed: &[String],
) -> CheckReport {
    let mut check = CheckReport {
        is_formal,
        ..CheckReport::default()
    };
    if manifest.get("format").and_then(Value::as_str) != Some(MANIFEST_FORMAT)
        || manifest.get("version").and_then(Value::as_u64) != Some(MANIFEST_VERSION)
    {
        check.problems.push(format!(
            "the manifest is not a {MANIFEST_FORMAT} version {MANIFEST_VERSION} file"
        ));
        return check;
    }
    let entries = manifest
        .get("bindings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let chars: Vec<char> = report.chars().collect();
    let mut cache = HashMap::new();
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut seen_artifacts = std::collections::HashSet::new();
    let mut last_end = 0usize;
    for (index, entry) in entries.iter().enumerate() {
        let at = format!("binding {}", index + 1);
        let text = |name: &str| entry.get(name).and_then(Value::as_str);
        let (Some(artifact), Some(sha256), Some(key), Some(format), Some(rendered)) = (
            text("artifact"),
            text("sha256"),
            text("key"),
            text("format"),
            text("rendered"),
        ) else {
            check
                .problems
                .push(format!("{at}: the manifest entry is incomplete"));
            continue;
        };
        let (Some(raw), Some(offset)) = (
            entry.get("raw").and_then(Value::as_f64),
            entry.get("offset").and_then(Value::as_u64),
        ) else {
            check.problems.push(format!(
                "{at}: the manifest entry has no raw value or offset"
            ));
            continue;
        };
        let offset = offset as usize;
        seen_artifacts.insert(artifact.to_owned());
        match load_value(&mut cache, read, artifact, key) {
            Err(error) => check
                .problems
                .push(format!("{at} ({artifact}#{key}): {error}")),
            Ok((current_sha, current)) => {
                if current_sha != sha256 {
                    check.problems.push(format!(
                        "{at} ({artifact}#{key}): the result file has changed since the report was rendered"
                    ));
                } else if current != raw {
                    check.problems.push(format!(
                        "{at} ({artifact}#{key}): the file holds {current}, the manifest says {raw}"
                    ));
                }
            }
        }
        match parse_format(format).and_then(|parsed| render(raw, parsed)) {
            Ok(expected) if expected == rendered => {}
            Ok(expected) => check.problems.push(format!(
                "{at}: {raw} in the format {format} is {expected}, not {rendered}"
            )),
            Err(error) => check.problems.push(format!("{at}: {error}")),
        }
        let length = rendered.chars().count();
        let found: String = chars
            .get(offset..offset + length)
            .map(|slice| slice.iter().collect())
            .unwrap_or_default();
        if found != rendered {
            check.problems.push(format!(
                "{at}: the report has {found:?} where the manifest says {rendered:?} (character {offset})"
            ));
        } else if offset < last_end {
            check
                .problems
                .push(format!("{at}: overlaps the binding before it"));
        } else {
            spans.push((offset, offset + length));
            last_end = offset + length;
        }
    }
    check.bindings = entries.len();
    check.artifacts = seen_artifacts.len();
    if is_formal {
        match unbound_numbers(report, &chars, &spans, allowed) {
            Ok(unbound) => check.unbound = unbound,
            Err(error) => check.problems.push(error),
        }
    }
    check
}

/// Every number in `chars` that lies outside all of `spans` and all allowed patterns.
fn unbound_numbers(
    report: &str,
    chars: &[char],
    spans: &[(usize, usize)],
    allowed: &[String],
) -> Result<Vec<UnboundNumber>, String> {
    if allowed.len() > MAX_ALLOWED_PATTERNS {
        return Err(format!(
            "at most {MAX_ALLOWED_PATTERNS} allowed patterns are accepted"
        ));
    }
    let mut patterns = Vec::new();
    for pattern in DEFAULT_ALLOWED
        .iter()
        .copied()
        .chain(allowed.iter().map(String::as_str))
    {
        patterns.push(compile(pattern)?);
    }
    // Byte offsets -> character offsets, once.
    let char_of_byte: Vec<usize> = {
        let mut map = vec![0usize; report.len() + 1];
        for (char_index, (byte, _)) in report.char_indices().enumerate() {
            map[byte] = char_index;
        }
        map[report.len()] = chars.len();
        map
    };
    let mut covered: Vec<(usize, usize)> = spans.to_vec();
    for pattern in &patterns {
        for found in pattern.find_iter(report) {
            covered.push((char_of_byte[found.start()], char_of_byte[found.end()]));
        }
    }
    let number = Regex::new(r"[0-9]+(?:[.,][0-9]+)*").expect("a fixed pattern");
    let mut unbound = Vec::new();
    for found in number.find_iter(report) {
        let (start, end) = (char_of_byte[found.start()], char_of_byte[found.end()]);
        if covered
            .iter()
            .any(|(from, to)| *from <= start && end <= *to)
        {
            continue;
        }
        let line = report[..found.start()].matches('\n').count() + 1;
        unbound.push(UnboundNumber {
            text: found.as_str().to_owned(),
            line,
        });
    }
    Ok(unbound)
}
