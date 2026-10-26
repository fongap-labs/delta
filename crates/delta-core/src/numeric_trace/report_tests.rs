use super::report::*;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashMap;

/// A result file as `statistics_record` writes it.
fn result_file(values: Value) -> Vec<u8> {
    let values: serde_json::Map<String, Value> = values
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| (key.clone(), json!({"value": value})))
        .collect();
    serde_json::to_vec(&json!({"format": "delta.result", "version": 1, "values": values})).unwrap()
}

struct Files(RefCell<HashMap<String, Vec<u8>>>, RefCell<usize>);

impl Files {
    fn new(files: &[(&str, Vec<u8>)]) -> Self {
        Files(
            RefCell::new(
                files
                    .iter()
                    .map(|(path, bytes)| (path.to_string(), bytes.clone()))
                    .collect(),
            ),
            RefCell::new(0),
        )
    }
    fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        *self.1.borrow_mut() += 1;
        self.0
            .borrow()
            .get(path)
            .cloned()
            .ok_or_else(|| format!("{path} was not found"))
    }
    fn set(&self, path: &str, bytes: Vec<u8>) {
        self.0.borrow_mut().insert(path.to_string(), bytes);
    }
    fn reads(&self) -> usize {
        *self.1.borrow()
    }
}

fn files() -> Files {
    Files::new(&[
        (
            "out/t.json",
            result_file(
                json!({"p_value": 0.0004918624151844586, "estimate": 2.675, "n": 12.0, "conf_low": 6.3572597973079095}),
            ),
        ),
        ("out/b.json", result_file(json!({"mean": 7.0}))),
    ])
}

fn render_ok(template: &str, files: &Files) -> Rendered {
    render_template(template, &|path| files.read(path))
        .unwrap_or_else(|problems| panic!("{problems:?}"))
}

fn manifest_of(rendered: &Rendered) -> Value {
    rendered.manifest("report.md", "template.md")
}

fn check(
    rendered: &Rendered,
    text: &str,
    files: &Files,
    is_formal: bool,
    allow: &[&str],
) -> CheckReport {
    let allow: Vec<String> = allow.iter().map(|pattern| pattern.to_string()).collect();
    check_report(
        text,
        &manifest_of(rendered),
        &|path| files.read(path),
        is_formal,
        &allow,
    )
}

#[test]
fn a_binding_becomes_the_formatted_number_and_the_manifest_says_where_it_came_from() {
    let files = files();
    let rendered = render_ok("# Results\n\nThe p value was {{result:out/t.json#p_value|%.4f}} with n = {{result:out/t.json#n|%.0f}}.\n", &files);
    assert_eq!(
        rendered.text,
        "# Results\n\nThe p value was 0.0005 with n = 12.\n"
    );
    assert_eq!(rendered.bindings.len(), 2);
    let first = &rendered.bindings[0];
    assert_eq!(
        (
            first.artifact.as_str(),
            first.key.as_str(),
            first.rendered.as_str()
        ),
        ("out/t.json", "p_value", "0.0005")
    );
    assert_eq!(first.raw, 0.0004918624151844586);
    assert_eq!(first.section.as_deref(), Some("Results"));
    assert_eq!(first.sha256.len(), 64);
    assert_eq!(
        rendered
            .text
            .chars()
            .skip(first.offset)
            .take(6)
            .collect::<String>(),
        "0.0005"
    );
    let manifest = manifest_of(&rendered);
    assert_eq!(manifest["format"], "delta.binding-manifest");
    assert_eq!(manifest["bindings"][0]["raw"], 0.0004918624151844586);
}

#[test]
fn rounding_is_half_up_on_the_shortest_decimal_and_offsets_count_characters() {
    let files = files();
    let rendered = render_ok(
        "估计值为 {{result:out/t.json#estimate|%.2f}}，平均 {{result:out/b.json#mean|%.1f}}。",
        &files,
    );
    assert_eq!(rendered.text, "估计值为 2.68，平均 7.0。");
    let first = &rendered.bindings[0];
    assert_eq!(
        first.offset, 5,
        "five characters come before it, not eight bytes"
    );
    assert_eq!(
        rendered
            .text
            .chars()
            .skip(first.offset)
            .take(4)
            .collect::<String>(),
        "2.68"
    );
}

#[test]
fn a_result_file_is_read_once_however_many_bindings_use_it() {
    let files = files();
    render_ok("{{result:out/t.json#n|%.0f}} {{result:out/t.json#estimate|%.1f}} {{result:out/t.json#p_value|%.3f}}", &files);
    assert_eq!(files.reads(), 1);
}

#[test]
fn every_problem_is_reported_and_nothing_is_rendered() {
    let files = files();
    let problems = render_template(
        "{{result:out/t.json#nope|%.2f}} {{result:out/missing.json#x|%.2f}} {{result:out/t.json#n|%.2e}} {{mean}} {{result:out/t.json#n|%.0f}} {{result:oops",
        &|path| files.read(path),
    )
    .unwrap_err();
    assert!(
        problems.iter().any(|p| p.contains("no value under nope")),
        "{problems:?}"
    );
    assert!(
        problems
            .iter()
            .any(|p| p.contains("missing.json was not found")),
        "{problems:?}"
    );
    assert!(
        problems.iter().any(|p| p.contains("unknown format")),
        "{problems:?}"
    );
    assert!(
        problems.iter().any(|p| p.contains("not closed")),
        "{problems:?}"
    );
    let not_json = Files::new(&[("out/x.json", b"plain".to_vec())]);
    let problems =
        render_template("{{result:out/x.json#a|%.1f}}", &|path| not_json.read(path)).unwrap_err();
    assert!(problems[0].contains("not a JSON result file"));
}

#[test]
fn a_template_with_no_bindings_renders_unchanged() {
    let files = files();
    let rendered = render_ok("Plain text, no numbers.\n", &files);
    assert_eq!(rendered.text, "Plain text, no numbers.\n");
    assert!(rendered.bindings.is_empty());
}

#[test]
fn a_report_that_matches_its_manifest_passes() {
    let files = files();
    let rendered = render_ok("# 1. Results\n\nThe estimate is {{result:out/t.json#estimate|%.2f}} (n = {{result:out/t.json#n|%.0f}}).\n\n1. First point\n", &files);
    let outcome = check(&rendered, &rendered.text, &files, true, &[]);
    assert!(
        outcome.passed(),
        "{:?} {:?}",
        outcome.problems,
        outcome.unbound
    );
    assert_eq!((outcome.bindings, outcome.artifacts), (2, 1));
}

#[test]
fn a_result_file_that_changed_since_rendering_is_found() {
    let files = files();
    let rendered = render_ok("p = {{result:out/t.json#p_value|%.4f}}", &files);
    files.set("out/t.json", result_file(json!({"p_value": 0.5})));
    let outcome = check(&rendered, &rendered.text, &files, true, &[]);
    assert!(
        outcome
            .problems
            .iter()
            .any(|p| p.contains("changed since the report was rendered")),
        "{:?}",
        outcome.problems
    );
    assert!(!outcome.passed());
}

#[test]
fn a_number_edited_by_hand_or_moved_in_the_report_is_found() {
    let files = files();
    let rendered = render_ok("Before. p = {{result:out/t.json#p_value|%.4f}}.", &files);
    let edited = rendered.text.replace("0.0005", "0.0001");
    let outcome = check(&rendered, &edited, &files, false, &[]);
    assert!(
        outcome
            .problems
            .iter()
            .any(|p| p.contains("where the manifest says")),
        "{:?}",
        outcome.problems
    );
    let moved = format!("A new sentence. {}", rendered.text);
    assert!(!check(&rendered, &moved, &files, false, &[]).passed());
    // A report shorter than the manifest expects does not panic.
    assert!(!check(&rendered, "", &files, false, &[]).passed());
}

#[test]
fn a_manifest_whose_rounding_or_value_is_wrong_is_found() {
    let files = files();
    let rendered = render_ok("{{result:out/t.json#estimate|%.2f}}", &files);
    let mut forged = manifest_of(&rendered);
    forged["bindings"][0]["rendered"] = json!("2.67");
    let outcome = check_report("2.67", &forged, &|p| files.read(p), false, &[]);
    assert!(
        outcome
            .problems
            .iter()
            .any(|p| p.contains("is 2.68, not 2.67")),
        "{:?}",
        outcome.problems
    );
    let mut other_value = manifest_of(&rendered);
    other_value["bindings"][0]["raw"] = json!(9.99);
    let outcome = check_report(&rendered.text, &other_value, &|p| files.read(p), false, &[]);
    assert!(
        outcome
            .problems
            .iter()
            .any(|p| p.contains("the manifest says 9.99")),
        "{:?}",
        outcome.problems
    );
    let mut gone = manifest_of(&rendered);
    gone["bindings"][0]["key"] = json!("gone");
    assert!(!check_report(&rendered.text, &gone, &|p| files.read(p), false, &[]).passed());
}

#[test]
fn a_manifest_that_is_not_one_is_refused() {
    let files = files();
    for manifest in [
        json!({}),
        json!({"format": "delta.binding-manifest", "version": 2, "bindings": []}),
        json!("text"),
    ] {
        let outcome = check_report("x", &manifest, &|p| files.read(p), true, &[]);
        assert!(
            outcome.problems[0].contains("delta.binding-manifest"),
            "{:?}",
            outcome.problems
        );
    }
    let rendered = render_ok("{{result:out/t.json#n|%.0f}}", &files);
    let mut incomplete = manifest_of(&rendered);
    incomplete["bindings"][0]
        .as_object_mut()
        .unwrap()
        .remove("sha256");
    assert!(
        check_report(&rendered.text, &incomplete, &|p| files.read(p), false, &[]).problems[0]
            .contains("incomplete")
    );
    let elevens = Files::new(&[("out/e.json", result_file(json!({"n": 11.0})))]);
    let rendered_twice = render_ok(
        "{{result:out/e.json#n|%.0f}}{{result:out/e.json#n|%.0f}}",
        &elevens,
    );
    assert_eq!(rendered_twice.text, "1111");
    let mut overlapping = manifest_of(&rendered_twice);
    overlapping["bindings"][1]["offset"] = json!(1);
    let outcome = check_report("1111", &overlapping, &|p| elevens.read(p), false, &[]);
    assert!(
        outcome.problems.iter().any(|p| p.contains("overlaps")),
        "{:?}",
        outcome.problems
    );
}

#[test]
fn a_formal_report_may_not_hold_a_number_outside_its_bindings() {
    let files = files();
    let rendered = render_ok("# Results\n\nWe saw {{result:out/t.json#n|%.0f}} samples. Shelf life is 24 months.\nSee table 3 on 2026-10-25.\n", &files);
    let outcome = check(&rendered, &rendered.text, &files, true, &[]);
    assert!(outcome.problems.is_empty(), "{:?}", outcome.problems);
    let numbers: Vec<(&str, usize)> = outcome
        .unbound
        .iter()
        .map(|n| (n.text.as_str(), n.line))
        .collect();
    assert_eq!(
        numbers,
        [("24", 3), ("3", 4), ("2026", 4), ("10", 4), ("25", 4)]
    );
    assert!(!outcome.passed());
}

#[test]
fn an_allowed_pattern_covers_a_condition_a_date_or_a_section_reference() {
    let files = files();
    let rendered = render_ok("Stored at 25 °C / 60 % RH.\nSee section 3.2 on 2026-10-25 for {{result:out/t.json#n|%.0f}} samples.\n", &files);
    let none = check(&rendered, &rendered.text, &files, true, &[]);
    assert!(!none.unbound.is_empty());
    let allowed = check(
        &rendered,
        &rendered.text,
        &files,
        true,
        &[
            r"25 °C / 60 % RH",
            r"section \d+(?:\.\d+)*",
            r"\d{4}-\d{2}-\d{2}",
        ],
    );
    assert!(
        allowed.passed(),
        "{:?} {:?}",
        allowed.problems,
        allowed.unbound
    );
}

#[test]
fn headings_and_numbered_lists_are_allowed_without_asking() {
    let files = files();
    let rendered = render_ok(
        "# 1 Introduction\n## 2.1. Method\n\n1. one\n2) two\n   3. three\n",
        &files,
    );
    let outcome = check(&rendered, &rendered.text, &files, true, &[]);
    assert!(outcome.passed(), "{:?}", outcome.unbound);
}

#[test]
fn an_informal_report_is_not_scanned_and_says_so() {
    let files = files();
    let rendered = render_ok(
        "Roughly 100 people, {{result:out/t.json#n|%.0f}} of them tested.",
        &files,
    );
    let outcome = check(&rendered, &rendered.text, &files, false, &[]);
    assert!(outcome.passed());
    assert!(outcome.unbound.is_empty());
    assert!(outcome.to_json()["note"]
        .as_str()
        .unwrap()
        .contains("Not a formal report"));
    assert!(check(&rendered, &rendered.text, &files, true, &[]).to_json()["note"].is_null());
}

#[test]
fn a_bad_or_oversized_allowed_pattern_is_a_problem_not_a_panic() {
    let files = files();
    let rendered = render_ok("text {{result:out/t.json#n|%.0f}}", &files);
    let bad = check(&rendered, &rendered.text, &files, true, &["("]);
    assert!(
        bad.problems.iter().any(|p| p.contains("not valid")),
        "{:?}",
        bad.problems
    );
    let long = "a".repeat(201);
    assert!(!check(&rendered, &rendered.text, &files, true, &[long.as_str()]).passed());
    let many: Vec<&str> = vec!["x"; 21];
    assert!(check(&rendered, &rendered.text, &files, true, &many)
        .problems
        .iter()
        .any(|p| p.contains("at most")));
}

#[test]
fn offsets_stay_right_when_the_report_has_chinese_text_before_and_between_numbers() {
    let files = files();
    let rendered = render_ok("\u{672c}\u{7814}\u{7a76}\u{5171} {{result:out/t.json#n|%.0f}} \u{4f8b}\u{ff0c}p \u{503c} {{result:out/t.json#p_value|%.3f}}\u{ff0c}\u{7ed3}\u{8bba}\u{6210}\u{7acb}\u{3002}", &files);
    assert_eq!(rendered.text, "本研究共 12 例，p 值 0.000，结论成立。");
    let outcome = check(&rendered, &rendered.text, &files, true, &[]);
    assert!(
        outcome.passed(),
        "{:?} {:?}",
        outcome.problems,
        outcome.unbound
    );
}

// --- the tools ---

fn host_call(
    workspace: &std::path::Path,
    tool: &str,
    arguments: Value,
) -> crate::runtime::ToolResult {
    use crate::runtime::{ToolCall, ToolExecutionContext, ToolExecutor};
    let host = crate::capability::CapabilityHost::product_defaults().unwrap();
    host.execute(
        &ToolCall {
            id: "c".into(),
            name: tool.into(),
            arguments,
        },
        &ToolExecutionContext {
            session_id: "s".into(),
            run_id: "r".into(),
            workspace: Some(workspace.to_string_lossy().to_string()),
            timeout: std::time::Duration::from_secs(10),
            cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            progress: std::sync::Arc::new(|_| {}),
            secrets: Default::default(),
            proxy: None,
        },
    )
}

fn workspace_with_results() -> tempfile::TempDir {
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(workspace.path().join("out")).unwrap();
    std::fs::write(
        workspace.path().join("out/t.json"),
        result_file(json!({"p_value": 0.0004918624151844586, "n": 12.0})),
    )
    .unwrap();
    std::fs::write(
        workspace.path().join("template.md"),
        "# Results\n\np = {{result:out/t.json#p_value|%.4f}}, n = {{result:out/t.json#n|%.0f}}\n",
    )
    .unwrap();
    workspace
}

/// Put what the render tool staged where the runtime would promote it.
fn promote(workspace: &std::path::Path, result: &crate::runtime::ToolResult) {
    for staged in &result.staged_artifacts {
        let target = workspace.join(&staged.relative_path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::copy(&staged.staging_path, target).unwrap();
    }
}

#[test]
fn the_render_tool_stages_a_report_and_its_manifest_and_the_check_tool_passes_them() {
    let workspace = workspace_with_results();
    let rendered = host_call(
        workspace.path(),
        "report_render",
        json!({"template_path": "template.md", "report_path": "out/report.md"}),
    );
    assert!(rendered.error.is_none(), "{:?}", rendered.error);
    let paths: Vec<&str> = rendered
        .staged_artifacts
        .iter()
        .map(|a| a.relative_path.as_str())
        .collect();
    assert_eq!(paths, ["out/report.md", "out/report.md.bindings.json"]);
    assert_eq!(
        std::fs::read_to_string(&rendered.staged_artifacts[0].staging_path).unwrap(),
        "# Results\n\np = 0.0005, n = 12\n"
    );
    assert_eq!(rendered.output["bindings"], 2);
    assert_eq!(
        rendered.output["manifest_path"],
        "out/report.md.bindings.json"
    );

    promote(workspace.path(), &rendered);
    let checked = host_call(
        workspace.path(),
        "report_check",
        json!({"report_path": "out/report.md"}),
    );
    assert!(checked.error.is_none(), "{:?}", checked.error);
    assert_eq!(checked.output["passed"], true, "{}", checked.output);
    assert_eq!(checked.output["bindings"], 2);
}

#[test]
fn the_check_tool_finds_a_hand_edited_number_an_added_number_and_a_changed_result_file() {
    let workspace = workspace_with_results();
    let rendered = host_call(
        workspace.path(),
        "report_render",
        json!({"template_path": "template.md", "report_path": "report.md"}),
    );
    promote(workspace.path(), &rendered);
    let report = workspace.path().join("report.md");
    let original = std::fs::read_to_string(&report).unwrap();

    std::fs::write(&report, original.replace("n = 12", "n = 13")).unwrap();
    let edited = host_call(
        workspace.path(),
        "report_check",
        json!({"report_path": "report.md"}),
    );
    assert_eq!(edited.output["passed"], false);
    assert!(!edited.output["problems"].as_array().unwrap().is_empty());

    std::fs::write(
        &report,
        format!("{original}\nAbout 40 more were excluded.\n"),
    )
    .unwrap();
    let added = host_call(
        workspace.path(),
        "report_check",
        json!({"report_path": "report.md"}),
    );
    assert_eq!(added.output["passed"], false);
    assert_eq!(added.output["unbound_numbers"][0]["text"], "40");
    let informal = host_call(
        workspace.path(),
        "report_check",
        json!({"report_path": "report.md", "formal": false}),
    );
    assert_eq!(informal.output["passed"], true);
    let allowed = host_call(
        workspace.path(),
        "report_check",
        json!({"report_path": "report.md", "allow": ["About 40 more"]}),
    );
    assert_eq!(allowed.output["passed"], true);

    std::fs::write(&report, &original).unwrap();
    std::fs::write(
        workspace.path().join("out/t.json"),
        result_file(json!({"p_value": 0.9, "n": 12.0})),
    )
    .unwrap();
    let changed = host_call(
        workspace.path(),
        "report_check",
        json!({"report_path": "report.md"}),
    );
    assert_eq!(changed.output["passed"], false);
}

#[test]
fn the_render_tool_writes_nothing_when_a_binding_cannot_be_resolved_and_refuses_bad_paths() {
    let workspace = workspace_with_results();
    std::fs::write(
        workspace.path().join("bad.md"),
        "{{result:out/t.json#nope|%.2f}}",
    )
    .unwrap();
    let failed = host_call(
        workspace.path(),
        "report_render",
        json!({"template_path": "bad.md", "report_path": "r.md"}),
    );
    assert!(
        failed
            .error
            .as_deref()
            .unwrap()
            .contains("no value under nope"),
        "{:?}",
        failed.error
    );
    assert!(failed.staged_artifacts.is_empty());
    for arguments in [
        json!({"template_path": "template.md", "report_path": "r.docx"}),
        json!({"template_path": "template.md", "report_path": "../r.md"}),
        json!({"template_path": "../template.md", "report_path": "r.md"}),
        json!({"template_path": "template.md"}),
    ] {
        assert!(
            host_call(workspace.path(), "report_render", arguments.clone())
                .error
                .is_some(),
            "{arguments}"
        );
    }
    assert!(host_call(
        workspace.path(),
        "report_check",
        json!({"report_path": "missing.md"})
    )
    .error
    .is_some());
}

#[test]
fn the_render_tool_asks_first_and_the_check_tool_does_not() {
    for registration in super::tools::registrations() {
        let asks = registration.tool_name == "report_render";
        assert_eq!(registration.metadata["requires_approval"], asks);
        assert_eq!(registration.workspace_write, asks);
    }
}

// --- the data-analysis skill's claims about reports ------------------------------------------

/// The result file the skill's template points at, as `statistics_record` flattens a t test.
fn ttest_files() -> Files {
    Files::new(&[(
        "out/ttest.json",
        result_file(json!({
            "groups.1.n": 12.0, "groups.1.mean": 5.125, "groups.2.n": 11.0, "groups.2.mean": 7.8,
            "estimate": -2.675, "conf_low": -4.1, "conf_high": -1.25, "p_value": 0.0000123,
            "conf_level": 0.95
        })),
    )])
}

/// The one Markdown block of the skill: its report template.
fn skill_template() -> &'static str {
    let skill = include_str!("../../skills/data-analysis/SKILL.md");
    let start = skill
        .find("```markdown\n")
        .expect("the skill has a template")
        + "```markdown\n".len();
    let end = start
        + skill[start..]
            .find("\n```")
            .expect("the template is closed");
    &skill[start..end]
}

#[test]
fn the_template_in_the_data_analysis_skill_renders_and_passes_a_formal_check() {
    let files = ttest_files();
    let rendered = render_ok(skill_template(), &files);
    let outcome = check(&rendered, &rendered.text, &files, true, &[]);
    assert!(
        outcome.passed(),
        "{:?} {:?}",
        outcome.problems,
        outcome.unbound
    );
    assert_eq!(outcome.bindings, 8);
}

#[test]
fn a_literal_threshold_or_level_fails_a_formal_check_until_it_is_allowed_or_bound() {
    let files = ttest_files();
    // The pitfall the skill lists: digits outside a binding are unbound numbers.
    let rendered = render_ok("# Result\n\np below 0.05 at 95% confidence.\n", &files);
    let outcome = check(&rendered, &rendered.text, &files, true, &[]);
    let texts: Vec<&str> = outcome.unbound.iter().map(|n| n.text.as_str()).collect();
    assert_eq!(texts, ["0.05", "95"]);

    // An allowed pattern for fixed wording of the author's own passes it.
    let allowed = check(
        &rendered,
        &rendered.text,
        &files,
        true,
        &[r"0\.05", "95 ?%"],
    );
    assert!(allowed.passed(), "{:?}", allowed.unbound);

    // The confidence level can also be bound, as the skill says.
    let bound = render_ok(
        "# Result\n\nConfidence level {{result:out/ttest.json#conf_level|%.2f}}.\n",
        &files,
    );
    assert!(check(&bound, &bound.text, &files, true, &[]).passed());
}

#[test]
fn a_very_small_p_value_shows_as_zero_at_four_decimals_but_not_at_six() {
    let files = ttest_files();
    let four = render_ok("{{result:out/ttest.json#p_value|%.4f}}", &files);
    assert_eq!(four.text, "0.0000");
    let six = render_ok("{{result:out/ttest.json#p_value|%.6f}}", &files);
    assert_eq!(six.text, "0.000012");
}
