use super::*;

fn workspace() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn path(dir: &tempfile::TempDir) -> String {
    dir.path().to_string_lossy().to_string()
}

#[test]
fn a_rule_is_for_the_tool_or_for_exactly_one_command() {
    assert_eq!(
        rule_for("write_file", &json!({"path": "a.md"})),
        Some(("write_file".to_owned(), None))
    );
    assert_eq!(
        rule_for("run_shell", &json!({"command": "git status"})),
        Some(("run_shell".to_owned(), Some("git status".to_owned())))
    );
    assert_eq!(rule_for("run_shell", &json!({"command": "  "})), None);
    assert_eq!(rule_for("run_shell", &json!({"command": 5})), None);
    let long = "x".repeat(2001);
    assert_eq!(rule_for("run_shell", &json!({"command": long})), None);
}

#[test]
fn a_tool_rule_covers_every_call_and_a_command_rule_only_its_command() {
    let state = workspace();
    let ws = workspace();
    add(
        state.path(),
        &path(&ws),
        "write_file",
        &json!({"path": "a.md"}),
    )
    .unwrap();
    add(
        state.path(),
        &path(&ws),
        "run_shell",
        &json!({"command": "git status"}),
    )
    .unwrap();
    let covered = |tool: &str, arguments: Value| covers(state.path(), &path(&ws), tool, &arguments);
    assert!(covered("write_file", json!({"path": "anything.md"})));
    assert!(covered("run_shell", json!({"command": "git status"})));
    assert!(!covered("run_shell", json!({"command": "git push"})));
    assert!(!covered("run_shell", json!({})));
    assert!(!covered("read_file", json!({})));
}

#[test]
fn a_rule_belongs_to_its_workspace() {
    let state = workspace();
    let (one, two) = (workspace(), workspace());
    add(state.path(), &path(&one), "write_file", &json!({})).unwrap();
    assert!(covers(state.path(), &path(&one), "write_file", &json!({})));
    assert!(!covers(state.path(), &path(&two), "write_file", &json!({})));
    assert!(
        !covers(state.path(), "", "write_file", &json!({})),
        "no workspace, no rule"
    );
    assert!(!covers(
        state.path(),
        "/no/such/dir",
        "write_file",
        &json!({})
    ));
}

#[test]
fn adding_twice_keeps_one_rule_and_revoking_restores_the_question() {
    let state = workspace();
    let ws = workspace();
    let first = add(state.path(), &path(&ws), "write_file", &json!({})).unwrap();
    let again = add(state.path(), &path(&ws), "write_file", &json!({})).unwrap();
    assert_eq!(first, again);
    assert_eq!(list(state.path(), &path(&ws)).unwrap().len(), 1);
    assert!(revoke(state.path(), &path(&ws), "write_file", None).unwrap());
    assert!(!covers(state.path(), &path(&ws), "write_file", &json!({})));
    assert!(!revoke(state.path(), &path(&ws), "write_file", None).unwrap());
    assert!(list(state.path(), &path(&ws)).unwrap().is_empty());
}

#[test]
fn revoking_a_command_rule_leaves_the_tool_rule_and_the_other_commands() {
    let state = workspace();
    let ws = workspace();
    for command in ["git status", "git diff"] {
        add(
            state.path(),
            &path(&ws),
            "run_shell",
            &json!({"command": command}),
        )
        .unwrap();
    }
    assert!(revoke(state.path(), &path(&ws), "run_shell", Some("git status")).unwrap());
    assert!(!covers(
        state.path(),
        &path(&ws),
        "run_shell",
        &json!({"command": "git status"})
    ));
    assert!(covers(
        state.path(),
        &path(&ws),
        "run_shell",
        &json!({"command": "git diff"})
    ));
    // A tool-wide revoke does not remove a command rule.
    assert!(!revoke(state.path(), &path(&ws), "run_shell", None).unwrap());
}

#[test]
fn a_call_that_can_not_be_a_rule_is_refused_and_a_workspace_is_needed() {
    let state = workspace();
    let ws = workspace();
    assert!(add(
        state.path(),
        &path(&ws),
        "run_shell",
        &json!({"command": ""})
    )
    .is_err());
    assert!(add(state.path(), "", "write_file", &json!({})).is_err());
}

#[test]
fn a_workspace_holds_a_bounded_number_of_rules() {
    let state = workspace();
    let ws = workspace();
    for n in 0..MAX_RULES_PER_WORKSPACE {
        add(
            state.path(),
            &path(&ws),
            "run_shell",
            &json!({"command": format!("echo {n}")}),
        )
        .unwrap();
    }
    let error = add(
        state.path(),
        &path(&ws),
        "run_shell",
        &json!({"command": "one more"}),
    )
    .unwrap_err();
    assert!(error.to_string().contains("revoke some"), "{error}");
}

#[test]
fn a_file_that_does_not_parse_covers_nothing_and_is_not_overwritten_by_a_read() {
    let state = workspace();
    let ws = workspace();
    std::fs::write(state.path().join("workspace_rules.json"), "{broken").unwrap();
    assert!(!covers(state.path(), &path(&ws), "write_file", &json!({})));
    assert!(list(state.path(), &path(&ws)).is_err());
    assert!(add(state.path(), &path(&ws), "write_file", &json!({})).is_err());
    assert_eq!(
        std::fs::read_to_string(state.path().join("workspace_rules.json")).unwrap(),
        "{broken"
    );
}

#[test]
fn creating_and_revoking_a_rule_are_audited() {
    let state = workspace();
    let ws = workspace();
    add(state.path(), &path(&ws), "write_file", &json!({})).unwrap();
    revoke(state.path(), &path(&ws), "write_file", None).unwrap();
    let log = std::fs::read(state.path().join("audit_events.db")).unwrap();
    let text = String::from_utf8_lossy(&log);
    assert!(text.contains("approval.rule"));
}
