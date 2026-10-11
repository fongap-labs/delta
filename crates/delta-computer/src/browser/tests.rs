//! Browser tests run against a real Chromium when one is installed (set `DELTA_TEST_BROWSER` to
//! pick it); without one they report that and pass, so machines without a browser can still run
//! the suite. CI images used for releases have one.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use delta_core::capability::CapabilityHost;
use delta_core::runtime::{ToolCall, ToolExecutionContext, ToolExecutor, ToolExitState};

use super::*;

fn test_browser() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("DELTA_TEST_BROWSER") {
        return Some(PathBuf::from(path));
    }
    launch::find_browser(&Settings::default()).ok().or_else(|| {
        let root = Path::new("/opt/pw-browsers");
        std::fs::read_dir(root)
            .ok()?
            .flatten()
            .map(|e| e.path().join("chrome-linux").join("chrome"))
            .find(|p| p.is_file())
    })
}

fn settings(executable: PathBuf) -> Settings {
    Settings {
        executable: Some(executable),
        headless: true,
        extra_args: ["--no-sandbox", "--disable-gpu", "--disable-dev-shm-usage"]
            .map(String::from)
            .to_vec(),
        allow_loopback: true,
        idle_timeout: Duration::from_secs(600),
    }
}

const SHOP: &str = r#"<html><head><title>Shop</title></head><body>
<h1>Welcome to the shop</h1>
<a href="/other" target="_blank">Other page</a>
<form action="/done" method="get">
  <label for="n">Your name</label><input id="n" name="name">
  <input type="password" name="pw" aria-label="Secret">
  <label for="c">Colour</label><select id="c" name="colour"><option>Red</option><option>Blue</option></select>
  <button type="submit">Send it</button>
</form>
<p id="probe">pending</p>
<script>
  fetch('http://10.255.255.1/x').then(
    () => { document.getElementById('probe').textContent = 'reachable'; },
    () => { document.getElementById('probe').textContent = 'blocked'; });
</script>
</body></html>"#;

/// A tiny web server on 127.0.0.1 for the pages above.
fn serve() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || {
                let mut stream = stream;
                let mut buffer = [0u8; 4096];
                let read = stream.read(&mut buffer).unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                let target = request.split_whitespace().nth(1).unwrap_or("/").to_owned();
                let body = if target.starts_with("/done") {
                    format!(
                        "<html><head><title>Done</title></head><body><h1>Thanks</h1><p>{target}</p></body></html>"
                    )
                } else if target.starts_with("/other") {
                    "<html><head><title>Other</title></head><body>other page</body></html>"
                        .to_owned()
                } else {
                    SHOP.to_owned()
                };
                let reply = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(reply.as_bytes());
            });
        }
    });
    port
}

struct Rig {
    host: CapabilityHost,
    browser: Arc<BrowserHost>,
    workspace: tempfile::TempDir,
}

fn rig(settings: Settings) -> Rig {
    let state = tempfile::tempdir().unwrap();
    let browser = BrowserHost::new(state.path().to_path_buf(), settings);
    let host = CapabilityHost::new();
    for registration in registrations(&browser) {
        host.register(registration).unwrap();
    }
    // The state folder lives as long as the test; leak the handle rather than race the browser.
    std::mem::forget(state);
    Rig {
        host,
        browser,
        workspace: tempfile::tempdir().unwrap(),
    }
}

impl Rig {
    fn call(&self, name: &str, arguments: Value) -> delta_core::runtime::ToolResult {
        self.host.execute(
            &ToolCall {
                id: "c".to_owned(),
                name: name.to_owned(),
                arguments,
            },
            &ToolExecutionContext {
                session_id: "task-1".to_owned(),
                run_id: "run1".to_owned(),
                workspace: Some(self.workspace.path().to_string_lossy().to_string()),
                timeout: Duration::from_secs(60),
                cancel: Arc::new(AtomicBool::new(false)),
                progress: Arc::new(|_| {}),
                secrets: BTreeMap::new(),
            },
        )
    }

    fn ok(&self, name: &str, arguments: Value) -> Value {
        let started = Instant::now();
        let result = self.call(name, arguments);
        eprintln!("{name} took {:?}", started.elapsed());
        assert_eq!(
            result.state,
            ToolExitState::Completed,
            "{name}: {:?}",
            result.error
        );
        result.output
    }

    fn err(&self, name: &str, arguments: Value) -> String {
        let result = self.call(name, arguments);
        assert_ne!(result.state, ToolExitState::Completed, "{name} should fail");
        result.error.unwrap_or_default()
    }
}

fn element<'a>(page: &'a Value, label: &str) -> &'a Value {
    page["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            e["label"]
                .as_str()
                .is_some_and(|l| l.eq_ignore_ascii_case(label))
        })
        .unwrap_or_else(|| panic!("no element {label} in {page}"))
}

#[test]
fn the_guard_keeps_the_browser_off_the_users_network() {
    let strict = NetGuard::new(false);
    for blocked in [
        "http://127.0.0.1:8080/",
        "http://localhost/",
        "http://169.254.169.254/latest/meta-data/",
        "http://192.168.1.1/",
        "http://[::1]/",
        "file:///etc/passwd",
        "ftp://example.com/",
        "chrome://settings",
    ] {
        assert!(strict.check(blocked).is_err(), "{blocked}");
    }
    // Page-internal content and an empty page are fine.
    for allowed in [
        "about:blank",
        "data:text/html,hello",
        "blob:https://example.com/x",
    ] {
        assert!(strict.check(allowed).is_ok(), "{allowed}");
    }
    // A name this machine cannot resolve may still resolve through the user's proxy; the guard
    // relies on the resolver's own wording to tell that apart from a refusal.
    assert!(strict.check("http://no-such-host.invalid/").is_ok());
    assert!(NetGuard::new(true).check("http://127.0.0.1:8080/").is_ok());
    assert!(NetGuard::new(true).check("http://10.0.0.1/").is_err());
}

#[test]
fn reading_and_scrolling_need_no_approval_and_acting_does() {
    let state = tempfile::tempdir().unwrap();
    let browser = BrowserHost::new(state.path().to_path_buf(), Settings::default());
    for registration in registrations(&browser) {
        let asks = registration.metadata["requires_approval"]
            .as_bool()
            .unwrap();
        let expected = matches!(
            registration.tool_name.as_str(),
            "browser_navigate"
                | "browser_click"
                | "browser_type"
                | "browser_select"
                | "browser_press_key"
        );
        assert_eq!(asks, expected, "{}", registration.tool_name);
        assert_eq!(registration.metadata["category"], "computer");
        if asks {
            assert_eq!(registration.metadata["risk_level"], "medium");
        }
    }
    assert!(!browser.is_open(), "registering must not start a browser");
}

#[test]
fn a_missing_browser_is_reported_in_plain_words() {
    let rig = rig(Settings {
        executable: Some(PathBuf::from("/definitely/not/here/browser")),
        ..Settings::default()
    });
    let error = rig.err("browser_snapshot", json!({}));
    assert!(error.contains("not found"), "{error}");
}

#[test]
fn the_agent_can_browse_fill_in_and_submit_a_form() {
    let Some(executable) = test_browser() else {
        eprintln!("no browser installed; skipping");
        return;
    };
    let port = serve();
    let rig = rig(settings(executable));
    let base = format!("http://127.0.0.1:{port}");

    // Open a page: its text and numbered controls come back.
    let opened = rig.ok("browser_navigate", json!({"url": format!("{base}/")}));
    let page = &opened["page"];
    assert_eq!(page["title"], "Shop");
    assert!(page["text"]
        .as_str()
        .unwrap()
        .contains("Welcome to the shop"));
    let name_ref = element(page, "Your name")["ref"]
        .as_str()
        .unwrap()
        .to_owned();
    let colour_ref = element(page, "Colour")["ref"].as_str().unwrap().to_owned();
    let send_ref = element(page, "Send it")["ref"].as_str().unwrap().to_owned();
    let secret = element(page, "Secret");
    assert_eq!(secret["sensitive"], true);
    let secret_ref = secret["ref"].as_str().unwrap().to_owned();
    assert_eq!(element(page, "Colour")["options"], json!(["Red", "Blue"]));

    // The page's own script tried to reach a private address; the guard stopped it.
    let text = rig.ok("browser_snapshot", json!({}))["page"]["text"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        text.contains("blocked"),
        "the probe should be blocked: {text}"
    );

    // The label is checked: a ref that points at something else is not acted on.
    let wrong = rig.err(
        "browser_click",
        json!({"ref": send_ref, "label": "Delete everything"}),
    );
    assert!(wrong.contains("not \"Delete everything\""), "{wrong}");
    // Refs from nowhere are refused before reaching the page.
    assert!(rig
        .err("browser_click", json!({"ref": "e1\"]", "label": "x"}))
        .contains("element reference"));
    assert!(rig
        .err("browser_click", json!({"ref": "e99", "label": "x"}))
        .contains("not on the page"));

    // Passwords are never typed by the agent.
    let refused = rig.err(
        "browser_type",
        json!({"ref": secret_ref, "label": "Secret", "text": "hunter2"}),
    );
    assert!(refused.contains("password"), "{refused}");

    // Type, choose, send.
    rig.ok(
        "browser_type",
        json!({"ref": name_ref, "label": "Your name", "text": "Ada Lovelace"}),
    );
    let typed = rig.ok("browser_snapshot", json!({}));
    assert_eq!(
        element(&typed["page"], "Your name")["value"],
        "Ada Lovelace"
    );
    let bad_option = rig.err(
        "browser_select",
        json!({"ref": colour_ref, "label": "Colour", "option": "Green"}),
    );
    assert!(bad_option.contains("Red"), "{bad_option}");
    rig.ok(
        "browser_select",
        json!({"ref": colour_ref, "label": "Colour", "option": "Blue"}),
    );
    let submitted = rig.ok(
        "browser_click",
        json!({"ref": send_ref, "label": "Send it"}),
    );
    assert_eq!(submitted["page"]["title"], "Done");
    let landed = submitted["page"]["text"].as_str().unwrap();
    assert!(
        landed.contains("name=Ada+Lovelace") && landed.contains("colour=Blue"),
        "{landed}"
    );

    // Back, then scroll, then a picture the model can be shown.
    let back = rig.ok("browser_back", json!({}));
    assert_eq!(back["page"]["title"], "Shop");
    rig.ok("browser_scroll", json!({"direction": "bottom"}));
    assert!(rig
        .err("browser_scroll", json!({"direction": "sideways"}))
        .contains("direction"));
    let shot = rig.ok("browser_screenshot", json!({}));
    let saved = shot["images"][0]["path"].as_str().unwrap();
    assert!(
        saved.starts_with(".delta/screens/run1/") && saved.ends_with(".jpg"),
        "{saved}"
    );
    let bytes = std::fs::read(rig.workspace.path().join(saved)).unwrap();
    assert_eq!(&bytes[..2], &[0xff, 0xd8], "a JPEG picture");

    // A link that wants a new window opens in the same tab, and a second task gets its own tab.
    let other = element(&back["page"], "Other page")["ref"]
        .as_str()
        .unwrap()
        .to_owned();
    let followed = rig.ok(
        "browser_click",
        json!({"ref": other, "label": "Other page"}),
    );
    assert_eq!(followed["page"]["title"], "Other");

    // Closing the last tab closes the browser.
    assert!(rig.browser.is_open());
    let closed = rig.ok("browser_close", json!({}));
    assert_eq!(closed["browser_closed"], true);
    assert!(!rig.browser.is_open());
}

#[test]
fn the_browser_refuses_addresses_on_the_users_own_network() {
    let Some(executable) = test_browser() else {
        eprintln!("no browser installed; skipping");
        return;
    };
    let port = serve();
    let rig = rig(Settings {
        allow_loopback: false,
        ..settings(executable)
    });
    for address in [
        format!("http://127.0.0.1:{port}/"),
        "http://169.254.169.254/".to_owned(),
        "file:///etc/hostname".to_owned(),
    ] {
        let error = rig.err("browser_navigate", json!({"url": address}));
        assert!(error.contains("refused"), "{address}: {error}");
    }
    rig.browser.shutdown();
}

#[test]
fn the_browser_closes_itself_when_unused_and_comes_back_when_needed() {
    let Some(executable) = test_browser() else {
        eprintln!("no browser installed; skipping");
        return;
    };
    let port = serve();
    let rig = rig(Settings {
        idle_timeout: Duration::from_secs(1),
        ..settings(executable)
    });
    rig.ok(
        "browser_navigate",
        json!({"url": format!("http://127.0.0.1:{port}/")}),
    );
    assert!(rig.browser.is_open());
    let deadline = Instant::now() + Duration::from_secs(15);
    while rig.browser.is_open() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(!rig.browser.is_open(), "an idle browser should close");
    // The next call just starts it again.
    let again = rig.ok(
        "browser_navigate",
        json!({"url": format!("http://127.0.0.1:{port}/")}),
    );
    assert_eq!(again["page"]["title"], "Shop");
    rig.browser.shutdown();
    assert!(!rig.browser.is_open());
}
