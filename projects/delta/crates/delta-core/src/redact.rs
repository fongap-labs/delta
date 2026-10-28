//! Credential scrubbing for everything the runtime persists as history (the approval audit table
//! and the run event ledger).
//!
//! This is the Rust side of one policy; the Python side is `packages/sanitize.py`. Both are tested
//! against `tests/fixtures/redaction_golden.json`, so a change to one that is not mirrored in the
//! other fails a test instead of drifting:
//!
//! - values under secret-shaped keys (`token`, `password`, `api_key`, ...) are replaced, at any
//!   depth;
//! - credential-bearing HTTP headers are replaced by name;
//! - body-ish keys (`body`, `content`, `html`) are replaced wholesale;
//! - strings lose credential query parameters of http(s) URLs;
//! - free text (a shell command, a tool result) loses credentials that announce themselves:
//!   `Authorization:`/`Cookie:` headers, `--password x` style flags, `?token=x` query parameters and
//!   well-known token shapes.
//!
//! What stays: tool names, stages, statuses, risk levels, paths, the *shape* of the arguments (key
//! names and value types) and every value that is not a credential.

use std::sync::OnceLock;

use regex::{Captures, Regex};
use serde_json::{Map, Value};

const REDACTED: &str = "[redacted]";
const REDACTED_BODY: &str = "[redacted body]";

const SECRET_KEY_MARKERS: [&str; 12] = [
    "token",
    "secret",
    "password",
    "api_key",
    "apikey",
    "access_token",
    "bot_token",
    "app_token",
    "refresh_token",
    "credential",
    "private_key",
    "raw",
];
const SENSITIVE_HEADERS: [&str; 5] = [
    "authorization",
    "proxy-authorization",
    "cookie",
    "set-cookie",
    "x-api-key",
];
const BODY_KEYS: [&str; 3] = ["body", "content", "html"];
const URL_CREDENTIAL_PARAMS: [&str; 12] = [
    "token",
    "access_token",
    "refresh_token",
    "api_key",
    "apikey",
    "key",
    "secret",
    "password",
    "credential",
    "sig",
    "signature",
    "auth",
];

/// True when a mapping key names something secret-shaped.
pub fn is_secret_key(key: &str) -> bool {
    let lk = key.to_lowercase();
    SECRET_KEY_MARKERS.iter().any(|marker| lk.contains(marker))
        || SENSITIVE_HEADERS.contains(&lk.as_str())
        || SENSITIVE_HEADERS
            .iter()
            .any(|h| lk.ends_with(h) || lk.ends_with(&format!("[{h}]")))
}

/// True when a mapping key holds free text that is redacted wholesale.
pub fn is_body_key(key: &str) -> bool {
    let lk = key.to_lowercase();
    BODY_KEYS
        .iter()
        .any(|b| lk == *b || lk.ends_with(&format!("_{b}")))
}

/// Recursively apply the policy to any JSON value.
pub fn redact_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            for (key, item) in map {
                let redacted = if is_secret_key(key) {
                    Value::String(REDACTED.to_string())
                } else if is_body_key(key) {
                    Value::String(REDACTED_BODY.to_string())
                } else {
                    redact_value(item)
                };
                out.insert(key.clone(), redacted);
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(redact_value).collect()),
        Value::String(text) => Value::String(redact_free_text(text)),
        other => other.clone(),
    }
}

/// Scrub a free-text field (a result preview, a reason, a resource): URL credentials first, then the
/// credentials that announce themselves inside text.
pub fn redact_free_text(text: &str) -> String {
    redact_text(&redact_url_credentials(text))
}

/// Redact a JSON document held as text. Text that is not JSON is treated as free text.
pub fn redact_json_text(text: &str) -> String {
    match serde_json::from_str::<Value>(text) {
        Ok(value) => redact_value(&value).to_string(),
        Err(_) => redact_free_text(text),
    }
}

// --- credentials inside free text -------------------------------------------------------------
//
// The patterns are the same text as in `packages/sanitize.py` and use no look-around.

fn regexes() -> &'static [Regex; 5] {
    static CELL: OnceLock<[Regex; 5]> = OnceLock::new();
    CELL.get_or_init(|| {
        let build = |pattern: &str| Regex::new(pattern).expect("valid redaction pattern");
        [
            // query credentials, with or without a scheme
            build(
                r#"(?i)([?&](?:token|access_token|refresh_token|api_key|apikey|key|secret|password|credential|sig|signature|auth)=)([^&\s"'#]+)"#,
            ),
            // `Authorization: Bearer abc`, `X-Api-Key: abc`
            build(
                r#"(?i)\b(authorization|proxy-authorization|x-api-key|api-key)(\s*[:=]\s*)((?:(?:bearer|basic|token)\s+)?[^\s"'`;&|]+)"#,
            ),
            // cookie values hold spaces and semicolons
            build(r#"(?i)\b(cookie|set-cookie)(\s*:\s*)([^\r\n"']+)"#),
            // `--password hunter2`, `--token=abc`, `-secret "a b"`
            build(
                r#"(?i)(--?(?:password|passwd|token|secret|api[-_]?key|access[-_]?key|credential)s?)(\s+|=)("[^"]*"|'[^']*'|[^\s"';&|-][^\s"';&|]*)"#,
            ),
            // well-known token shapes
            build(
                r"\bsk-(?:ant-|proj-)?[A-Za-z0-9_-]{20,}|\bgh[pousr]_[A-Za-z0-9]{36,}|\bgithub_pat_[A-Za-z0-9_]{40,}|\bAKIA[0-9A-Z]{16}\b|\bxox[baprs]-[A-Za-z0-9-]{10,}|\bAIza[0-9A-Za-z_-]{35}|\bnvapi-[A-Za-z0-9_-]{20,}|\bhf_[A-Za-z0-9]{30,}|\beyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}",
            ),
        ]
    })
}

fn replace_secret(text: &str, pattern: &Regex, secret_group: usize) -> String {
    pattern
        .replace_all(text, |caps: &Captures| {
            let whole = caps.get(0).map_or("", |m| m.as_str());
            let secret = caps.get(secret_group).map_or("", |m| m.as_str());
            if secret.starts_with("[redacted") {
                return whole.to_string();
            }
            let mut out = String::new();
            for index in 1..secret_group {
                out.push_str(caps.get(index).map_or("", |m| m.as_str()));
            }
            out.push_str(REDACTED);
            out
        })
        .into_owned()
}

/// Scrub credentials that announce themselves inside free text. Idempotent.
pub fn redact_text(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    let [query, auth, cookie, flag, shapes] = regexes();
    let mut out = replace_secret(value, query, 2);
    out = replace_secret(&out, auth, 3);
    out = replace_secret(&out, cookie, 3);
    out = replace_secret(&out, flag, 3);
    shapes.replace_all(&out, REDACTED).into_owned()
}

// --- URL query parameters -----------------------------------------------------------------------

/// Python `urllib.parse.quote`: unreserved characters and `safe` pass, everything else is
/// percent-encoded as UTF-8 bytes.
fn py_quote(text: &str, safe: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        let c = byte as char;
        if c.is_ascii_alphanumeric() || "_.-~".contains(c) || (byte < 0x80 && safe.contains(c)) {
            out.push(c);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Strip credential-bearing query parameters from an http(s) URL string; anything else is returned
/// unchanged. Mirrors `redact_url_credentials` in `packages/sanitize.py`.
pub fn redact_url_credentials(value: &str) -> String {
    let (before_fragment, fragment) = match value.split_once('#') {
        Some((head, tail)) => (head, Some(tail)),
        None => (value, None),
    };
    let (before_query, query) = match before_fragment.split_once('?') {
        Some((head, tail)) => (head, tail),
        None => return value.to_string(),
    };
    let Some((scheme, rest)) = before_query.split_once("://") else {
        return value.to_string();
    };
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return value.to_string();
    }
    let (netloc, path) = match rest.find('/') {
        Some(slash) => (&rest[..slash], &rest[slash..]),
        None => (rest, ""),
    };
    if netloc.is_empty() {
        return value.to_string();
    }
    let pairs: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if pairs.is_empty() {
        return value.to_string();
    }
    let mut is_changed = false;
    let scrubbed: Vec<String> = pairs
        .iter()
        .map(|(k, v)| {
            let hide = URL_CREDENTIAL_PARAMS.contains(&k.to_lowercase().as_str());
            is_changed |= hide && v != REDACTED;
            let shown = if hide { REDACTED } else { v.as_str() };
            format!("{}={}", py_quote(k, ""), py_quote(shown, "[]"))
        })
        .collect();
    if !is_changed {
        return value.to_string();
    }
    let mut out = format!("{scheme}://{netloc}{path}?{}", scrubbed.join("&"));
    if let Some(fragment) = fragment {
        out.push('#');
        out.push_str(fragment);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn key_rules_match_the_policy() {
        for key in [
            "token",
            "Access_Token",
            "x-api-key",
            "headers[cookie]",
            "request.authorization",
            "raw_args",
        ] {
            assert!(is_secret_key(key), "{key}");
        }
        for key in ["path", "command", "tool", "level", "exit_code"] {
            assert!(!is_secret_key(key), "{key}");
        }
        assert!(is_body_key("body") && is_body_key("result_body") && is_body_key("HTML"));
        assert!(!is_body_key("bodyguard") && !is_body_key("contents"));
    }

    #[test]
    fn redaction_is_idempotent_and_keeps_the_shape() {
        let value = json!({"command": "curl -H 'Authorization: Bearer abc' x?token=1", "n": 3, "path": "C:/a b/c.txt"});
        let once = redact_value(&value);
        assert_eq!(redact_value(&once), once);
        assert_eq!(once["n"], json!(3));
        assert_eq!(once["path"], json!("C:/a b/c.txt"));
        assert!(!once.to_string().contains("abc"));
    }

    #[test]
    fn text_without_credentials_is_returned_unchanged() {
        for text in [
            "",
            "ls -la /tmp --port 8080",
            "tokens are great",
            "Keyboard: us",
            "see https://example.com/a?page=2#top",
        ] {
            assert_eq!(redact_text(text), text);
            assert_eq!(redact_url_credentials(text), text);
        }
    }

    #[test]
    fn url_rewriting_keeps_everything_but_the_credential() {
        assert_eq!(
            redact_url_credentials(
                "https://example.com/callback?code=abc&access_token=secret&state=xyz#frag"
            ),
            "https://example.com/callback?code=abc&access_token=[redacted]&state=xyz#frag"
        );
        assert_eq!(
            redact_url_credentials("ftp://h/?token=x"),
            "ftp://h/?token=x"
        );
        assert_eq!(
            redact_url_credentials("https://h/p?a=b%20c&key=k"),
            "https://h/p?a=b%20c&key=[redacted]"
        );
    }

    #[test]
    fn json_text_is_parsed_and_other_text_is_scrubbed_as_text() {
        let redacted = redact_json_text(r#"{"password":"p","note":"fine"}"#);
        assert!(
            redacted.contains("[redacted]")
                && redacted.contains("fine")
                && !redacted.contains("\"p\"")
        );
        assert_eq!(
            redact_json_text("--token abc rest"),
            "--token [redacted] rest"
        );
    }
}
