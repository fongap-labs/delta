use super::policy::{
    capture_allowed, decide, is_sensitive, tier_of, Action, Control, Decision, Program, RunBudget,
    SessionGrants, Stop, Tier, MAX_ACTIONS_PER_RUN,
};

fn notepad() -> Program {
    Program::new(r"C:\Windows\System32\notepad.exe", "无标题 - 记事本")
}

fn button(name: &str) -> Action {
    Action::Click {
        control: Control {
            name: name.to_owned(),
            kind: "Button".to_owned(),
            is_password: false,
        },
    }
}

#[test]
fn program_names_are_compared_by_file_name_in_any_case() {
    assert_eq!(
        Program::new(r"C:\Apps\NOTEPAD.EXE", "").process,
        "notepad.exe"
    );
    assert_eq!(Program::new("/usr/bin/Gedit", "").process, "gedit");
}

#[test]
fn shells_settings_and_registry_are_refused() {
    for exe in [
        "cmd.exe",
        "PowerShell.exe",
        "SystemSettings.exe",
        "regedit.exe",
        "SecHealthUI.exe",
    ] {
        let program = Program::new(exe, "");
        assert_eq!(tier_of(&program), Tier::Refused, "{exe}");
        assert!(matches!(
            decide(&program, &button("确定"), &SessionGrants::default()),
            Decision::Refuse(_)
        ));
    }
}

#[test]
fn refusal_holds_even_when_the_user_has_granted_the_program() {
    let program = Program::new("cmd.exe", "");
    let mut grants = SessionGrants::default();
    grants.grant(&program);
    assert!(matches!(
        decide(&program, &button("确定"), &grants),
        Decision::Refuse(_)
    ));
}

#[test]
fn password_managers_are_per_action_by_process_name() {
    let vault = Program::new(r"C:\Program Files\1Password\1Password.exe", "1Password");
    assert_eq!(tier_of(&vault), Tier::PerAction);
    let mut grants = SessionGrants::default();
    grants.grant(&vault);
    // Even with a grant, a plain click needs confirmation.
    assert!(matches!(
        decide(&vault, &button("打开"), &grants),
        Decision::Confirm(_)
    ));
}

#[test]
fn banking_windows_are_per_action_by_title_whatever_the_program() {
    let browser = Program::new("msedge.exe", "招商银行网上银行 - Microsoft Edge");
    assert_eq!(tier_of(&browser), Tier::PerAction);
    let plain = Program::new("msedge.exe", "新闻 - Microsoft Edge");
    assert_eq!(tier_of(&plain), Tier::Ordinary);
}

#[test]
fn ordinary_program_needs_a_grant_before_any_action() {
    let program = notepad();
    let grants = SessionGrants::default();
    assert_eq!(
        decide(&program, &button("打开"), &grants),
        Decision::NeedsGrant
    );
}

#[test]
fn granted_program_runs_ordinary_actions_without_asking_again() {
    let program = notepad();
    let mut grants = SessionGrants::default();
    grants.grant(&program);
    assert_eq!(decide(&program, &button("打开"), &grants), Decision::Allow);
    assert_eq!(
        decide(
            &program,
            &Action::Type {
                control: Control {
                    name: "正文".into(),
                    kind: "Edit".into(),
                    is_password: false
                },
                text: "你好".into(),
            },
            &grants
        ),
        Decision::Allow
    );
}

#[test]
fn sensitive_clicks_are_confirmed_even_in_a_granted_program() {
    let program = notepad();
    let mut grants = SessionGrants::default();
    grants.grant(&program);
    for name in ["发送", "提交", "删除", "确认付款", "Send", "Delete all"] {
        assert!(
            matches!(
                decide(&program, &button(name), &grants),
                Decision::Confirm(_)
            ),
            "{name}"
        );
    }
}

#[test]
fn sensitive_keys_are_confirmed_and_ordinary_keys_are_not() {
    let program = notepad();
    let mut grants = SessionGrants::default();
    grants.grant(&program);
    let enter = Action::Key {
        combo: "Enter".into(),
    };
    assert!(matches!(
        decide(&program, &enter, &grants),
        Decision::Confirm(_)
    ));
    let save = Action::Key {
        combo: "ctrl+s".into(),
    };
    assert_eq!(decide(&program, &save, &grants), Decision::Allow);
}

#[test]
fn coordinate_clicks_are_always_confirmed() {
    let program = notepad();
    let mut grants = SessionGrants::default();
    grants.grant(&program);
    assert!(matches!(
        decide(&program, &Action::Coordinates { x: 10, y: 20 }, &grants),
        Decision::Confirm(_)
    ));
}

#[test]
fn password_fields_are_refused_even_in_a_granted_program() {
    let program = notepad();
    let mut grants = SessionGrants::default();
    grants.grant(&program);
    let typed = Action::Type {
        control: Control {
            name: "密码".into(),
            kind: "Edit".into(),
            is_password: true,
        },
        text: "secret".into(),
    };
    assert!(matches!(
        decide(&program, &typed, &grants),
        Decision::Refuse(_)
    ));
}

#[test]
fn a_grant_is_for_one_program_and_can_be_revoked() {
    let granted = notepad();
    let other = Program::new("wordpad.exe", "");
    let mut grants = SessionGrants::default();
    grants.grant(&granted);
    assert!(grants.is_granted(&granted));
    assert_eq!(
        decide(&other, &button("打开"), &grants),
        Decision::NeedsGrant
    );

    grants.revoke(&granted);
    assert_eq!(
        decide(&granted, &button("打开"), &grants),
        Decision::NeedsGrant
    );
}

#[test]
fn clearing_the_session_withdraws_every_grant() {
    let program = notepad();
    let mut grants = SessionGrants::default();
    grants.grant(&program);
    grants.clear();
    assert!(!grants.is_granted(&program));
}

#[test]
fn screenshots_stop_while_a_banking_or_password_window_is_visible() {
    let vault = Program::new("bitwarden.exe", "");
    let notes = notepad();
    assert!(capture_allowed(&[notes.clone()]));
    assert!(!capture_allowed(&[notes, vault]));
    assert!(capture_allowed(&[]));
}

#[test]
fn sensitive_word_check_ignores_case_and_matches_inside_names() {
    assert!(is_sensitive("Confirm Payment"));
    assert!(is_sensitive("提交订单"));
    assert!(!is_sensitive("保存"));
}

#[test]
fn a_run_stops_at_the_action_limit() {
    let mut budget = RunBudget::default();
    for _ in 0..MAX_ACTIONS_PER_RUN {
        assert!(budget.begin_action().is_ok());
    }
    assert_eq!(budget.begin_action(), Err(Stop::ActionLimit));
}

#[test]
fn the_same_action_failing_three_times_in_a_row_stops_the_run() {
    let mut budget = RunBudget::default();
    let action = button("打开");
    assert!(budget.record(&action, false).is_ok());
    assert!(budget.record(&action, false).is_ok());
    assert!(matches!(
        budget.record(&action, false),
        Err(Stop::RepeatedFailures { .. })
    ));
}

#[test]
fn a_different_action_or_a_success_breaks_the_failure_streak() {
    let mut budget = RunBudget::default();
    let first = button("打开");
    let second = button("保存");
    assert!(budget.record(&first, false).is_ok());
    assert!(budget.record(&first, false).is_ok());
    assert!(budget.record(&second, false).is_ok());
    assert!(budget.record(&first, false).is_ok());
    assert!(budget.record(&first, true).is_ok());
    assert!(budget.record(&first, false).is_ok());
    assert!(budget.record(&first, false).is_ok());
}
