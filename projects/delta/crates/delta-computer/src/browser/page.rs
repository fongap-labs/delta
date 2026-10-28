//! What the tools do to one open tab: read it, click, type, scroll, go back.

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::cdp::Cdp;
use super::script;

const CALL_TIMEOUT: Duration = Duration::from_secs(20);
/// The longest a page is given to finish loading after an action before the tools carry on.
const SETTLE_LIMIT: Duration = Duration::from_secs(12);
pub const MAX_TYPED_CHARS: usize = 4_000;

pub const STALE: &str = "That element is not on the page any more. Call browser_snapshot to get \
    fresh element references.";

/// One tab, with the connection used to drive it.
pub struct Ctx<'a> {
    pub cdp: &'a Cdp,
    pub session: &'a str,
    pub cancelled: &'a dyn Fn() -> bool,
}

/// A reference such as `e12`, as printed by a snapshot. Anything else is refused before it can
/// reach a script.
pub fn reference(arguments: &Value) -> Result<String, String> {
    let text = arguments
        .get("ref")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let digits = text.strip_prefix('e').unwrap_or("");
    if digits.is_empty() || digits.len() > 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(
            "ref must be an element reference from the latest snapshot, like e3".to_owned(),
        );
    }
    Ok(text.to_owned())
}

pub fn label_arg(arguments: &Value) -> Result<String, String> {
    let label = arguments
        .get("label")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if label.is_empty() {
        return Err(
            "label is required: the name of the element as the snapshot shows it, so the user can \
            see what is being touched"
                .to_owned(),
        );
    }
    Ok(label.to_owned())
}

fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Whether the element found under a reference is the one the model named. Close matches count (a
/// shortened label, a label with an icon's text added), different elements do not.
pub fn label_matches(actual: &str, wanted: &str) -> bool {
    let (actual, wanted) = (normalize(actual), normalize(wanted));
    if actual == wanted {
        return true;
    }
    let shorter = actual.chars().count().min(wanted.chars().count());
    shorter >= 2 && (actual.contains(&wanted) || wanted.contains(&actual))
}

impl Ctx<'_> {
    pub fn send(&self, method: &str, params: Value) -> Result<Value, String> {
        self.cdp.call(
            method,
            params,
            Some(self.session),
            CALL_TIMEOUT,
            self.cancelled,
        )
    }

    fn pause(&self, duration: Duration) -> Result<(), String> {
        let end = Instant::now() + duration;
        while Instant::now() < end {
            if (self.cancelled)() {
                return Err("cancelled".to_owned());
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        Ok(())
    }

    /// Run a script in the page and return its value. A page that is in the middle of navigating
    /// has no script context for a moment; that is retried.
    pub fn eval(&self, expression: &str) -> Result<Value, String> {
        let mut last = String::new();
        for _ in 0..5 {
            match self.send(
                "Runtime.evaluate",
                json!({
                    "expression": expression, "returnByValue": true,
                    "awaitPromise": true, "timeout": 8000
                }),
            ) {
                Ok(result) => {
                    if let Some(details) = result.get("exceptionDetails") {
                        let text = details
                            .pointer("/exception/description")
                            .or_else(|| details.get("text"))
                            .and_then(Value::as_str)
                            .unwrap_or("the page script failed");
                        return Err(format!("the page script failed: {text}"));
                    }
                    return Ok(result
                        .pointer("/result/value")
                        .cloned()
                        .unwrap_or(Value::Null));
                }
                Err(error)
                    if error.contains("context")
                        || error.contains("Inspected target navigated") =>
                {
                    last = error;
                    self.pause(Duration::from_millis(300))?;
                }
                Err(error) => return Err(error),
            }
        }
        Err(last)
    }

    /// Give the page a moment to react to an action and to finish loading what it started.
    pub fn settle(&self) -> Result<(), String> {
        self.pause(Duration::from_millis(350))?;
        let end = Instant::now() + SETTLE_LIMIT;
        while Instant::now() < end {
            // A failed read just means the page is between documents; look again.
            if let Ok(Value::String(state)) = self.eval(script::ready_state()) {
                if state == "complete" {
                    break;
                }
            }
            self.pause(Duration::from_millis(120))?;
        }
        self.pause(Duration::from_millis(200))
    }

    pub fn snapshot(&self) -> Result<Value, String> {
        let page = self.eval(&script::snapshot())?;
        if !page.is_object() {
            return Err("the page could not be read".to_owned());
        }
        Ok(page)
    }

    /// The element under `reference`, checked to be the one named by `label`.
    pub fn locate(&self, reference: &str, label: &str) -> Result<Value, String> {
        let info = self.eval(&script::locate(reference))?;
        if info.get("found").and_then(Value::as_bool) != Some(true) {
            return Err(STALE.to_owned());
        }
        let actual = info.get("label").and_then(Value::as_str).unwrap_or("");
        if !label_matches(actual, label) {
            return Err(format!(
                "Element {reference} is \"{actual}\", not \"{label}\". The page has probably \
                changed; call browser_snapshot and look again."
            ));
        }
        Ok(info)
    }

    pub fn mouse_click(&self, x: i64, y: i64) -> Result<(), String> {
        self.send(
            "Input.dispatchMouseEvent",
            json!({"type": "mouseMoved", "x": x, "y": y}),
        )?;
        for kind in ["mousePressed", "mouseReleased"] {
            self.send(
                "Input.dispatchMouseEvent",
                json!({"type": kind, "x": x, "y": y, "button": "left", "clickCount": 1}),
            )?;
        }
        Ok(())
    }

    pub fn press(&self, key: &str) -> Result<(), String> {
        let (code, virtual_code, text) = key_codes(key)
            .ok_or_else(|| format!("key \"{key}\" is not supported; use one of: {KEY_NAMES}"))?;
        let mut down = json!({
            "type": if text.is_empty() { "rawKeyDown" } else { "keyDown" },
            "key": key, "code": code, "windowsVirtualKeyCode": virtual_code,
        });
        if !text.is_empty() {
            down["text"] = json!(text);
        }
        self.send("Input.dispatchKeyEvent", down)?;
        self.send(
            "Input.dispatchKeyEvent",
            json!({"type": "keyUp", "key": key, "code": code, "windowsVirtualKeyCode": virtual_code}),
        )?;
        Ok(())
    }

    pub fn go_back(&self) -> Result<(), String> {
        let history = self.send("Page.getNavigationHistory", json!({}))?;
        let index = history
            .get("currentIndex")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let entry = if index > 0 {
            history
                .get("entries")
                .and_then(|e| e.get((index - 1) as usize))
                .and_then(|e| e.get("id"))
                .and_then(Value::as_i64)
        } else {
            None
        };
        let Some(entry) = entry else {
            return Err("there is no earlier page to go back to".to_owned());
        };
        self.send("Page.navigateToHistoryEntry", json!({"entryId": entry}))?;
        Ok(())
    }
}

const KEY_NAMES: &str = "Enter, Tab, Escape, Backspace, Delete, Space, ArrowUp, ArrowDown, \
    ArrowLeft, ArrowRight, PageUp, PageDown, Home, End";

/// `(code, windows virtual key code, text)` of the keys the tool offers. No letters and no
/// modifiers: typing goes through `browser_type`, and shortcuts such as Ctrl+W are not offered.
fn key_codes(key: &str) -> Option<(&'static str, i64, &'static str)> {
    Some(match key {
        "Enter" => ("Enter", 13, "\r"),
        "Tab" => ("Tab", 9, ""),
        "Escape" => ("Escape", 27, ""),
        "Backspace" => ("Backspace", 8, ""),
        "Delete" => ("Delete", 46, ""),
        "Space" | " " => ("Space", 32, " "),
        "ArrowUp" => ("ArrowUp", 38, ""),
        "ArrowDown" => ("ArrowDown", 40, ""),
        "ArrowLeft" => ("ArrowLeft", 37, ""),
        "ArrowRight" => ("ArrowRight", 39, ""),
        "PageUp" => ("PageUp", 33, ""),
        "PageDown" => ("PageDown", 34, ""),
        "Home" => ("Home", 36, ""),
        "End" => ("End", 35, ""),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_are_plain_element_numbers() {
        assert_eq!(reference(&json!({"ref": "e12"})).unwrap(), "e12");
        assert_eq!(reference(&json!({"ref": " e3 "})).unwrap(), "e3");
        for bad in ["", "12", "e", "e1\"]", "e12345", "x1", "e-1"] {
            assert!(reference(&json!({"ref": bad})).is_err(), "{bad}");
        }
        assert!(reference(&json!({})).is_err());
    }

    #[test]
    fn labels_match_loosely_but_not_across_elements() {
        assert!(label_matches("Sign in", "sign in"));
        assert!(label_matches("  Sign   in ", "Sign in"));
        assert!(label_matches("Sign in with Google", "Sign in"));
        assert!(label_matches("Search", "Search the web"));
        assert!(!label_matches("Delete account", "Sign in"));
        assert!(!label_matches("Buy now", "Add to cart"));
        // One letter is not enough to call two labels the same element.
        assert!(!label_matches("OK", "O"));
    }
}
