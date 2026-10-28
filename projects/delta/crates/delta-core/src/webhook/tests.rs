use super::*;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;

/// A one-request HTTP server on loopback. It answers with `status_line` and reports the raw
/// request it received, so a test can read the headers and the body.
pub(crate) fn serve_once(status_line: &'static str) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/hook", listener.local_addr().unwrap());
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut raw = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            let count = stream.read(&mut buffer).unwrap_or(0);
            if count == 0 {
                break;
            }
            raw.extend_from_slice(&buffer[..count]);
            let text = String::from_utf8_lossy(&raw);
            if let Some((head, body)) = text.split_once("\r\n\r\n") {
                let length = head
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if body.len() >= length {
                    break;
                }
            }
        }
        let response = format!(
            "{status_line}\r\nContent-Length: 0\r\nLocation: http://127.0.0.1:9/elsewhere\r\nConnection: close\r\n\r\n"
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = sender.send(String::from_utf8_lossy(&raw).into_owned());
    });
    (url, receiver)
}

fn completion() -> Completion {
    Completion {
        session_id: "s1".to_owned(),
        source: "chat",
        title: Some("Quarterly report".to_owned()),
        summary: "Done. The report is ready.".to_owned(),
        artifacts: vec![("out/q3.docx".to_owned(), "q3.docx".to_owned())],
        finished_at: 1_700_000_000,
    }
}

fn body_of(raw: &str) -> Value {
    serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap()
}

fn configured(
    level: EgressLevel,
    url: &str,
    is_enabled: bool,
    is_detailed: bool,
) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    egress::set_default_level(temp.path(), level, true).unwrap();
    set_settings(temp.path(), Some(url), is_enabled, is_detailed).unwrap();
    temp
}

#[test]
fn urls_that_could_leak_or_cannot_be_posted_to_are_refused() {
    for (url, needle) in [
        ("ftp://example.com/x", "http"),
        ("not a url", "not valid"),
        ("https://user:secret@example.com/x", "user name"),
        ("https://:secret@example.com/x", "user name"),
        ("file:///etc/passwd", "http"),
    ] {
        let error = validate_url(url).unwrap_err();
        assert!(error.contains(needle), "{url}: {error}");
    }
    let long = format!("https://example.com/{}", "a".repeat(2100));
    assert!(validate_url(&long).unwrap_err().contains("longer"));
    assert!(validate_url("https://example.com/hook?token=abc").is_ok());
    assert!(validate_url("http://localhost:8080/").is_ok());
}

#[test]
fn settings_round_trip_and_reject_turning_on_without_a_url() {
    let temp = tempfile::tempdir().unwrap();
    assert_eq!(settings(temp.path()).unwrap()["is_enabled"], false);
    assert!(set_settings(temp.path(), None, true, false)
        .unwrap_err()
        .contains("URL"));
    assert!(set_settings(temp.path(), Some("ftp://x/"), false, false).is_err());
    let saved = set_settings(
        temp.path(),
        Some(" http://localhost:8080/hook "),
        true,
        true,
    )
    .unwrap();
    assert_eq!(saved["url"], "http://localhost:8080/hook");
    let read = settings(temp.path()).unwrap();
    assert_eq!(read["is_enabled"], true);
    assert_eq!(read["include_details"], true);
    let cleared = set_settings(temp.path(), Some(""), false, false).unwrap();
    assert!(cleared["url"].is_null());
}

#[test]
fn a_setting_file_that_does_not_parse_is_an_error_and_is_not_overwritten_by_a_read() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("webhook.json"), "{nope").unwrap();
    assert!(settings(temp.path()).is_err());
    assert!(matches!(
        deliver(temp.path(), "", &completion()),
        Outcome::Skipped { .. }
    ));
    assert_eq!(
        std::fs::read_to_string(temp.path().join("webhook.json")).unwrap(),
        "{nope"
    );
}

#[test]
fn the_summary_is_collapsed_cut_on_a_character_boundary_and_marked() {
    assert_eq!(summarize("  one\n\ntwo   three ", 100), "one two three");
    assert_eq!(summarize("abcdefghij", 5), "abcde…");
    // Multi-byte characters are counted as characters, so the cut never splits one.
    assert_eq!(summarize("完成了报告和图表的制作", 4), "完成了报…");
    assert_eq!(summarize("", 5), "");
}

#[test]
fn a_post_carries_no_content_unless_the_user_asked_for_details() {
    let minimal = payload(&completion(), false);
    assert_eq!(minimal["event"], "turn_completed");
    assert_eq!(minimal["session_id"], "s1");
    assert_eq!(minimal["source"], "chat");
    assert_eq!(minimal["artifact_count"], 1);
    for key in ["title", "summary", "artifacts"] {
        assert!(
            minimal.get(key).is_none(),
            "{key} must not be in a minimal post"
        );
    }
    let detailed = payload(&completion(), true);
    assert_eq!(detailed["title"], "Quarterly report");
    assert_eq!(detailed["summary"], "Done. The report is ready.");
    assert_eq!(detailed["artifacts"][0]["path"], "out/q3.docx");
}

#[test]
fn a_detailed_post_names_at_most_twenty_files_and_still_counts_them_all() {
    let mut many = completion();
    many.artifacts = (0..30)
        .map(|n| (format!("f{n}.md"), format!("f{n}.md")))
        .collect();
    let body = payload(&many, true);
    assert_eq!(body["artifact_count"], 30);
    assert_eq!(body["artifacts"].as_array().unwrap().len(), 20);
}

#[test]
fn a_local_webhook_is_posted_to_under_private_only() {
    let (url, received) = serve_once("HTTP/1.1 200 OK");
    let temp = configured(EgressLevel::PrivateOnly, &url, true, false);
    let outcome = deliver(temp.path(), "", &completion());
    assert_eq!(
        outcome,
        Outcome::Sent {
            host: "127.0.0.1".to_owned(),
            status: 200
        }
    );
    let raw = received.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(raw.starts_with("POST /hook "), "{raw}");
    assert!(raw
        .to_ascii_lowercase()
        .contains("content-type: application/json"));
    let body = body_of(&raw);
    assert_eq!(body["event"], "turn_completed");
    assert!(body.get("summary").is_none());
}

#[test]
fn details_are_sent_only_when_turned_on() {
    let (url, received) = serve_once("HTTP/1.1 204 No Content");
    let temp = configured(EgressLevel::PrivateOnly, &url, true, true);
    assert!(matches!(
        deliver(temp.path(), "", &completion()),
        Outcome::Sent { status: 204, .. }
    ));
    let body = body_of(&received.recv_timeout(Duration::from_secs(5)).unwrap());
    assert_eq!(body["summary"], "Done. The report is ready.");
    assert_eq!(body["artifacts"][0]["name"], "q3.docx");
}

#[test]
fn a_switched_off_webhook_does_nothing_and_records_nothing() {
    let (url, _received) = serve_once("HTTP/1.1 200 OK");
    let temp = configured(EgressLevel::PrivateOnly, &url, false, false);
    assert_eq!(deliver(temp.path(), "", &completion()), Outcome::Off);
    let temp = tempfile::tempdir().unwrap();
    assert_eq!(deliver(temp.path(), "", &completion()), Outcome::Off);
}

#[test]
fn a_public_host_is_refused_under_private_only_and_nothing_is_sent() {
    // An IP literal is classified without DNS; 8.8.8.8 is public.
    let temp = configured(
        EgressLevel::PrivateOnly,
        "http://8.8.8.8:9/hook",
        true,
        true,
    );
    match deliver(temp.path(), "", &completion()) {
        Outcome::Skipped { host, reason } => {
            assert_eq!(host, "8.8.8.8");
            assert!(
                reason.contains("Not sent") && reason.contains("private network"),
                "{reason}"
            );
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_public_host_that_is_not_approved_is_refused_under_approved_endpoints() {
    let temp = configured(
        EgressLevel::ApprovedEndpoints,
        "http://8.8.8.8:9/hook",
        true,
        false,
    );
    assert!(matches!(
        deliver(temp.path(), "", &completion()),
        Outcome::Skipped { .. }
    ));
}

#[test]
fn ask_each_time_sends_nothing_because_a_webhook_cannot_ask() {
    let (url, received) = serve_once("HTTP/1.1 200 OK");
    let temp = configured(EgressLevel::AskEachTime, &url, true, false);
    match deliver(temp.path(), "", &completion()) {
        Outcome::Skipped { reason, .. } => assert!(reason.contains("can not ask"), "{reason}"),
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert!(received.recv_timeout(Duration::from_millis(300)).is_err());
}

#[test]
fn nothing_is_sent_before_the_user_has_chosen_an_egress_level() {
    let (url, received) = serve_once("HTTP/1.1 200 OK");
    let temp = tempfile::tempdir().unwrap();
    set_settings(temp.path(), Some(&url), true, false).unwrap();
    match deliver(temp.path(), "", &completion()) {
        Outcome::Skipped { reason, .. } => assert!(reason.contains("Choose where"), "{reason}"),
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert!(received.recv_timeout(Duration::from_millis(300)).is_err());
}

#[test]
fn a_redirect_is_not_followed_and_counts_as_a_failure() {
    let (url, received) = serve_once("HTTP/1.1 302 Found");
    let temp = configured(EgressLevel::PrivateOnly, &url, true, false);
    match deliver(temp.path(), "", &completion()) {
        Outcome::Failed { error, .. } => assert!(error.contains("302"), "{error}"),
        other => panic!("expected a failure, got {other:?}"),
    }
    assert!(received.recv_timeout(Duration::from_secs(5)).is_ok());
}

#[test]
fn a_server_error_is_a_failure_and_a_dead_server_does_not_hang() {
    let (url, _received) = serve_once("HTTP/1.1 500 Internal Server Error");
    let temp = configured(EgressLevel::PrivateOnly, &url, true, false);
    assert!(matches!(
        deliver(temp.path(), "", &completion()),
        Outcome::Failed { .. }
    ));
    // Nothing listens on this port any more.
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let dead = format!("http://{}/hook", closed.local_addr().unwrap());
    drop(closed);
    let temp = configured(EgressLevel::PrivateOnly, &dead, true, false);
    let started = std::time::Instant::now();
    assert!(matches!(
        deliver(temp.path(), "", &completion()),
        Outcome::Failed { .. }
    ));
    assert!(started.elapsed() < Duration::from_secs(15));
}

#[test]
fn a_test_post_works_before_the_webhook_is_switched_on_and_says_when_there_is_no_url() {
    let temp = tempfile::tempdir().unwrap();
    match test(temp.path(), "") {
        Outcome::Skipped { reason, .. } => assert!(reason.contains("save a URL"), "{reason}"),
        other => panic!("{other:?}"),
    }
    let (url, received) = serve_once("HTTP/1.1 200 OK");
    let temp = configured(EgressLevel::PrivateOnly, &url, false, false);
    assert!(matches!(
        test(temp.path(), ""),
        Outcome::Sent { status: 200, .. }
    ));
    let body = body_of(&received.recv_timeout(Duration::from_secs(5)).unwrap());
    assert_eq!(body["event"], "test");
}

#[test]
fn every_attempt_is_written_to_the_audit_log_without_the_body() {
    let (url, _received) = serve_once("HTTP/1.1 200 OK");
    let temp = configured(EgressLevel::PrivateOnly, &url, true, true);
    assert!(matches!(
        deliver(temp.path(), "", &completion()),
        Outcome::Sent { .. }
    ));
    let log = std::fs::read(temp.path().join("audit_events.db")).unwrap();
    let text = String::from_utf8_lossy(&log);
    assert!(text.contains("notification.webhook"));
    assert!(
        !text.contains("Quarterly report"),
        "the body must not be audited"
    );
    assert!(!text.contains("The report is ready"));
}

#[test]
fn the_outcome_json_names_the_result() {
    assert_eq!(Outcome::Off.to_json()["result"], "off");
    assert_eq!(
        Outcome::Sent {
            host: "h".into(),
            status: 200
        }
        .to_json()["status"],
        200
    );
    assert_eq!(
        Outcome::Skipped {
            host: "h".into(),
            reason: "r".into()
        }
        .to_json()["reason"],
        "r"
    );
    assert_eq!(
        Outcome::Failed {
            host: "h".into(),
            error: "e".into()
        }
        .to_json()["error"],
        "e"
    );
}
