//! Which proxy, if any, an outbound request goes through — and which hosts bypass it.
//!
//! Three modes, because "no proxy" and "the system's proxy" are different promises:
//!
//! - `direct` — this request must leave the machine unproxied. Used to guarantee that a
//!   direct attempt is genuinely direct, so a proxy fallback means something.
//! - `system` — read what the OS says and apply it for this call only. Never persisted:
//!   a transient corporate or VPN proxy must not become sticky configuration.
//! - `manual` — the values the user typed. The only mode that writes durable config.
//!
//! The default is `system`, which is deliberate. It is the only value that makes all four
//! exits converge on a superset of today's behaviour: the browser already follows the
//! system proxy, `delta-stt` already probes it on failure, and the web/provider doors
//! previously ignored it entirely. Starting at `direct` would silently *remove* proxy
//! support from two doors that have it.

use serde_json::{json, Map, Value};
use ureq::Proxy;

/// Prefix the Windows registry (and most users) store, without a scheme. ureq requires one.
const SCHEME_FALLBACK: &str = "http://";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyMode {
    Direct,
    System,
    Manual,
}

impl ProxyMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::System => "system",
            Self::Manual => "manual",
        }
    }

    fn parse(raw: Option<&Value>) -> Self {
        match raw.and_then(Value::as_str) {
            Some("direct") => Self::Direct,
            Some("manual") => Self::Manual,
            // Absent, unknown, or "system": the system's own configuration is the
            // conservative reading of "unset" for a product whose other doors
            // already follow it.
            _ => Self::System,
        }
    }
}

/// The proxy configuration as persisted in product preferences.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxySettings {
    pub mode: ProxyMode,
    /// Manual HTTP proxy, e.g. `http://127.0.0.1:7890` (credentials allowed).
    pub http: String,
    /// Manual HTTPS proxy. Falls back to `http` when empty, which is what nearly
    /// every local proxy expects.
    pub https: String,
    /// Comma-separated hosts that must never be proxied, e.g. `localhost,*.internal`.
    pub no_proxy: String,
}

/// The decision for one outbound request, keeping the mode's promise explicit: a
/// `system` request may *probe* (direct first, proxy on failure), while a `manual`
/// request must not fall back — the user asked for that exact route. Hosts outside
/// this crate (the voice-model downloader) consume this so every exit follows one
/// setting without having to re-derive it from the raw mode + addresses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxyRoute {
    Direct,
    System,
    Manual(String),
}

impl ProxySettings {
    /// Read the policy out of product preferences. Tolerates every malformed shape by
    /// falling back rather than erroring: a bad proxy string must not break boot, it
    /// must surface as a validation error on the field that owns it.
    pub fn from_prefs(prefs: &Map<String, Value>) -> Self {
        Self {
            mode: ProxyMode::parse(prefs.get("proxy_mode")),
            http: string_field(prefs, "proxy_http"),
            https: string_field(prefs, "proxy_https"),
            no_proxy: string_field(prefs, "proxy_no_proxy"),
        }
    }

    /// Apply a patch from the settings UI. Switching away from `manual` drops the
    /// addresses: keeping stale ones around would make switching back surprising.
    pub fn write_prefs(prefs: &mut Map<String, Value>, patch: &Value) -> Result<Self, String> {
        let mut settings = Self::from_prefs(prefs);
        if let Some(mode) = patch.get("mode").and_then(Value::as_str) {
            settings.mode = match mode {
                "direct" => ProxyMode::Direct,
                "system" => ProxyMode::System,
                "manual" => ProxyMode::Manual,
                other => {
                    return Err(format!(
                        "proxy mode must be direct, system or manual (got {other})"
                    ))
                }
            };
            if settings.mode != ProxyMode::Manual {
                settings.http.clear();
                settings.https.clear();
                settings.no_proxy.clear();
            }
        }
        for (key, field) in [("http", &mut settings.http), ("https", &mut settings.https)] {
            if let Some(value) = patch.get(key).and_then(Value::as_str) {
                *field = normalize_proxy_url(value);
            }
        }
        // The bypass list is HOSTS, not URLs: prefixing "http://" here would turn
        // "localhost" into "http://localhost" and stop it from ever matching a host.
        if let Some(value) = patch.get("no_proxy").and_then(Value::as_str) {
            settings.no_proxy = value
                .split(',')
                .map(str::trim)
                .collect::<Vec<_>>()
                .join(",");
        }
        // A manual config with no usable address is a configuration error, not a silent
        // fallback to direct: the user asked for a proxy and would otherwise wonder why
        // traffic still leaves the machine unproxied.
        if settings.mode == ProxyMode::Manual && settings.proxy_url("https").is_none() {
            return Err(
                "manual proxy mode needs at least an HTTP or HTTPS proxy address".to_string(),
            );
        }
        prefs.insert("proxy_mode".to_string(), json!(settings.mode.as_str()));
        prefs.insert("proxy_http".to_string(), json!(settings.http));
        prefs.insert("proxy_https".to_string(), json!(settings.https));
        prefs.insert("proxy_no_proxy".to_string(), json!(settings.no_proxy));
        Ok(settings)
    }

    /// What the settings UI shows: the persisted values plus what the system mode
    /// currently resolves to, so the user can see the effective route without guessing.
    pub fn snapshot(prefs: &Map<String, Value>) -> Value {
        let settings = Self::from_prefs(prefs);
        json!({
            "mode": settings.mode.as_str(),
            "http": settings.http,
            "https": settings.https,
            "no_proxy": settings.no_proxy,
            // Read-only echo of the detected system proxy (never persisted).
            "system_detected": detect_system_proxy(),
        })
    }

    /// The proxy URL that applies to `url`, or `None` when this request goes direct.
    pub fn proxy_for(&self, url: &str) -> Option<String> {
        let host = host_of(url)?;
        if self.bypasses(&host) {
            return None;
        }
        let scheme = url.split(':').next().unwrap_or("https");
        self.proxy_url(scheme)
    }

    /// The transport-level proxy for `url`, or `None` when it goes direct.
    ///
    /// A URL ureq cannot parse is treated as no proxy rather than as a request
    /// failure: the settings surface rejects a manual entry with no usable
    /// address, so a parse failure here means an address written by an older
    /// build or edited on disk, and failing every request over it would take the
    /// app down for a cosmetic problem. Direct is the pre-proxy behaviour.
    pub fn ureq_proxy(&self, url: &str) -> Option<Proxy> {
        let value = self.proxy_for(url)?;
        Proxy::new(&value).ok()
    }

    /// The decision for one outbound request: the mode's promise, and for `manual`
    /// the resolved address. `system` means "probe at call time" (direct first,
    /// system proxy only if the host is unreachable at the transport level).
    pub fn route_for(&self, url: &str) -> ProxyRoute {
        match self.mode {
            ProxyMode::Direct => ProxyRoute::Direct,
            ProxyMode::System => ProxyRoute::System,
            ProxyMode::Manual => match self.proxy_for(url) {
                Some(value) => ProxyRoute::Manual(value),
                None => ProxyRoute::Direct, // host on bypass list
            },
        }
    }

    /// Whether `host` is on the NO_PROXY list. Matching follows the de-facto standard:
    /// an exact host, or a suffix match at a label boundary. `*` alone bypasses
    /// everything, and `*.example.com` means "example.com and its subdomains".
    pub fn bypasses(&self, host: &str) -> bool {
        let host = host.trim().to_ascii_lowercase();
        if self.no_proxy.trim().is_empty() {
            return false;
        }
        self.no_proxy
            .split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .any(|entry| {
                // An entry may carry a port (`host:port`); the host is what matches.
                let entry = entry
                    .split(':')
                    .next()
                    .unwrap_or(entry)
                    .to_ascii_lowercase();
                // Leading `.` (`.internal`) and `*.` (`*.corp.test`) are both spellings
                // of "this domain and below"; strip to the bare domain before matching.
                let entry = entry.strip_prefix("*.").unwrap_or(&entry);
                let entry = entry.trim_start_matches('.');
                if entry.is_empty() || entry == "*" || entry == host {
                    return entry == host || entry == "*";
                }
                host.len() > entry.len() && host.ends_with(&format!(".{entry}"))
            })
    }

    /// The configured URL for a scheme, or `None`. An HTTPS request may reuse the HTTP
    /// proxy (both pointing at the same process is the norm for local tooling), never
    /// the other way round.
    fn proxy_url(&self, scheme: &str) -> Option<String> {
        let raw = if scheme == "http" {
            self.http.as_str()
        } else if self.https.trim().is_empty() {
            self.http.as_str()
        } else {
            self.https.as_str()
        };
        let raw = raw.trim();
        if raw.is_empty() {
            return None;
        }
        Some(normalize_proxy_url(raw))
    }
}

/// Host of a URL: lowercased, without scheme, credentials or port.
pub fn host_of(url: &str) -> Option<String> {
    let after_scheme = url.split("://").nth(1)?;
    let authority = after_scheme.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next().unwrap_or(authority);
    // An IPv6 literal is bracketed precisely so the port can be split off: `[::1]:8443`
    // → `[::1]`. A bare `::1` (no brackets, no port) would be ambiguous, and URLs always
    // bracket it, so the bracket is what we look for.
    let host = match host.strip_prefix('[') {
        Some(rest) => rest.split(']').next().unwrap_or(rest),
        None => host.split(':').next().unwrap_or(host),
    };
    if host.is_empty() {
        return None;
    }
    Some(host.to_ascii_lowercase())
}

/// Give a proxy URL a scheme. A bare `host:port` is the form the Windows registry and
/// most users type, and ureq rejects a schemeless proxy outright.
pub fn normalize_proxy_url(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        String::new()
    } else if raw.contains("://") {
        raw.to_string()
    } else {
        format!("{SCHEME_FALLBACK}{raw}")
    }
}

fn string_field(prefs: &Map<String, Value>, key: &str) -> String {
    prefs
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// Read the OS's proxy configuration. Env vars first (the portable contract), then the
/// Windows per-user registry value. macOS/Linux have no equivalent single source
/// without shelling out to `scutil`/`gsettings`, which is deliberately not done: a GUI
/// process spawning console tools is its own problem, and the answer would be
/// unreliable anyway. The result is for ONE call and is never written back.
pub fn detect_system_proxy() -> Option<String> {
    for var in [
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "HTTP_PROXY",
        "http_proxy",
    ] {
        if let Ok(value) = std::env::var(var) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(normalize_proxy_url(value));
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(value) = windows_internet_proxy() {
            return Some(normalize_proxy_url(&value));
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn windows_internet_proxy() -> Option<String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    // GUI processes spawning console tools (reg.exe) pop a visible console window
    // unless the child is created without one.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = Command::new("reg")
        .args(["query", KEY, "/v", "ProxyServer"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().find(|line| line.contains("ProxyServer"))?;
    let value = line.split_whitespace().nth(2)?;
    // A per-protocol value (`http=…;https=…;ftp=…`) is not a URL; taking the first
    // server is the best reading available without a full IE parser.
    let first = value.split(';').next().unwrap_or(value);
    let first = first.split('=').last().unwrap_or(first);
    if first.is_empty() {
        None
    } else {
        Some(first.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefs(pairs: &[(&str, Value)]) -> Map<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect()
    }

    fn manual(http: &str, no_proxy: &str) -> ProxySettings {
        ProxySettings {
            mode: ProxyMode::Manual,
            http: http.to_string(),
            https: String::new(),
            no_proxy: no_proxy.to_string(),
        }
    }

    #[test]
    fn unset_preferences_mean_system_mode_not_direct() {
        assert_eq!(
            ProxySettings::from_prefs(&Map::new()).mode,
            ProxyMode::System
        );
    }

    #[test]
    fn a_bare_host_port_gets_a_scheme() {
        assert_eq!(
            normalize_proxy_url("127.0.0.1:7890"),
            "http://127.0.0.1:7890"
        );
        assert_eq!(
            normalize_proxy_url("  http://proxy:3128  "),
            "http://proxy:3128"
        );
        assert_eq!(
            normalize_proxy_url("socks5://proxy:1080"),
            "socks5://proxy:1080"
        );
        assert_eq!(normalize_proxy_url("   "), "");
    }

    #[test]
    fn https_may_reuse_the_http_proxy_and_never_the_reverse() {
        let settings = manual("http://proxy:3128", "");
        assert_eq!(
            settings.proxy_for("https://example.com/a"),
            Some("http://proxy:3128".to_string())
        );
        // The reverse would send plaintext traffic through a tunnel-only proxy.
        let mut split = manual("", "");
        split.https = "http://proxy:3128".to_string();
        assert_eq!(split.proxy_for("http://example.com/a"), None);
        assert_eq!(
            split.proxy_for("https://example.com/a"),
            Some("http://proxy:3128".to_string())
        );
    }

    #[test]
    fn no_proxy_matches_exact_hosts_and_label_boundaries_only() {
        let settings = manual(
            "http://proxy:3128",
            "localhost, .internal , *.corp.test:8080",
        );
        assert!(settings.bypasses("localhost"));
        assert!(settings.bypasses("db.internal"));
        assert!(settings.bypasses("a.b.internal"));
        // "internal" alone must not match "notinternal.example".
        assert!(!settings.bypasses("notinternal.example"));
        assert!(!settings.bypasses("example.com"));
        // A port in an entry is ignored: a host entry covers the host.
        assert!(settings.bypasses("api.corp.test"));
        // `*` alone is the only spelling that bypasses everything; `*.corp.test` does not.
        assert!(settings.bypasses("corp.test"));
        assert!(!settings.bypasses("other.test"));
    }

    #[test]
    fn a_star_bypasses_everything() {
        let settings = manual("http://proxy:3128", "*");
        assert!(settings.proxy_for("https://anywhere.test/").is_none());
    }

    #[test]
    fn manual_mode_without_an_address_is_refused_rather_than_silently_direct() {
        let mut prefs = Map::new();
        let error =
            ProxySettings::write_prefs(&mut prefs, &json!({"mode": "manual", "http": "  "}))
                .unwrap_err();
        assert!(error.contains("at least an HTTP or HTTPS proxy address"));
        // The refused patch left no half-written state behind.
        assert!(prefs.get("proxy_mode").is_none());
    }

    #[test]
    fn switching_away_from_manual_drops_the_addresses() {
        let mut prefs = prefs(&[
            ("proxy_mode", json!("manual")),
            ("proxy_http", json!("http://proxy:3128")),
        ]);
        ProxySettings::write_prefs(&mut prefs, &json!({"mode": "system"})).unwrap();
        let settings = ProxySettings::from_prefs(&prefs);
        assert_eq!(settings.mode, ProxyMode::System);
        assert!(
            settings.http.is_empty(),
            "a stale address must not survive a mode change"
        );
    }

    #[test]
    fn host_of_reads_past_credentials_and_ports() {
        assert_eq!(
            host_of("https://user:pw@Example.com:8443/x").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            host_of("http://127.0.0.1:8080").as_deref(),
            Some("127.0.0.1")
        );
        assert_eq!(host_of("not a url"), None);
        // Brackets are the URL's disambiguator, not part of the host: `[::1]:8443`
        // normalises to `::1`, which is what a NO_PROXY entry is written as.
        assert_eq!(host_of("https://[::1]:8443/x").as_deref(), Some("::1"));
    }

    #[test]
    fn snapshot_echoes_the_detected_system_proxy_without_persisting_it() {
        let prefs = prefs(&[("proxy_mode", json!("system"))]);
        let snapshot = ProxySettings::snapshot(&prefs);
        assert_eq!(snapshot["mode"], "system");
        assert!(snapshot.get("system_detected").is_some());
        // Whatever the environment said, nothing was written back into prefs.
        assert!(prefs.get("system_detected").is_none());
    }

    #[test]
    fn a_persisted_round_trip_keeps_the_manual_addresses() {
        let mut prefs = Map::new();
        ProxySettings::write_prefs(
            &mut prefs,
            &json!({"mode": "manual", "http": "127.0.0.1:7890", "https": "http://127.0.0.1:7890", "no_proxy": "localhost"}),
        )
        .unwrap();
        let settings = ProxySettings::from_prefs(&prefs);
        assert_eq!(settings.mode, ProxyMode::Manual);
        assert_eq!(settings.http, "http://127.0.0.1:7890");
        assert_eq!(settings.https, "http://127.0.0.1:7890");
        assert_eq!(settings.no_proxy, "localhost");
    }

    #[test]
    fn route_for_maps_settings_correctly() {
        // direct mode -> always Direct
        let mut prefs = Map::new();
        ProxySettings::write_prefs(&mut prefs, &json!({"mode": "direct"})).unwrap();
        let s = ProxySettings::from_prefs(&prefs);
        assert_eq!(s.route_for("https://example.com"), ProxyRoute::Direct);
        // system mode -> always System (probe)
        let mut prefs = Map::new();
        ProxySettings::write_prefs(&mut prefs, &json!({"mode": "system"})).unwrap();
        let s = ProxySettings::from_prefs(&prefs);
        assert_eq!(s.route_for("http://anywhere.test"), ProxyRoute::System);
        // manual mode with set address -> Manual(url)
        let mut prefs = Map::new();
        ProxySettings::write_prefs(
            &mut prefs,
            &json!({
                "mode": "manual",
                "http": "http://proxy.local:3128",
                "https": "http://proxy.local:3128",
                "no_proxy": ""
            }),
        )
        .unwrap();
        let s = ProxySettings::from_prefs(&prefs);
        assert_eq!(
            s.route_for("https://foo.com"),
            ProxyRoute::Manual("http://proxy.local:3128".to_string())
        );
        // manual mode with empty proxy addresses -> Direct (no usable address)
        let mut prefs = Map::new();
        prefs.insert("proxy_mode".to_string(), json!("manual"));
        prefs.insert("proxy_http".to_string(), json!(""));
        prefs.insert("proxy_https".to_string(), json!(""));
        prefs.insert("proxy_no_proxy".to_string(), json!(""));
        let s = ProxySettings::from_prefs(&prefs);
        assert_eq!(s.route_for("https://example.com"), ProxyRoute::Direct);
        // manual mode but host matches bypass list -> Direct (no proxy)
        let mut prefs = Map::new();
        ProxySettings::write_prefs(
            &mut prefs,
            &json!({
                "mode": "manual",
                "http": "http://proxy.local:3128",
                "https": "http://proxy.local:3128",
                "no_proxy": "localhost, *.internal"
            }),
        )
        .unwrap();
        let s = ProxySettings::from_prefs(&prefs);
        assert_eq!(s.route_for("http://localhost/foo"), ProxyRoute::Direct);
        assert_eq!(s.route_for("http://internal/foo"), ProxyRoute::Direct);
    }
}
