//! The user's list of search engines: which are on, in what order, with which keys, plus the
//! engines they added themselves (a browser-style "add search engine": a name and a URL with `%s`).
//!
//! The whole list, keys included, is one credential-store value that only the search tool is
//! granted. What the interface gets back (`public_view`) says whether an engine has a key, never
//! the key.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::search::{Engine, EngineKind};

/// Built-in engines in their default order. Brave and Tavily need a key, so they start off.
pub const BUILTIN: [(&str, &str, bool); 5] = [
    ("google", "Google", true),
    ("bing", "Bing", true),
    ("duckduckgo", "DuckDuckGo", true),
    ("brave", "Brave Search", false),
    ("tavily", "Tavily", false),
];
const MAX_ENGINES: usize = 20;
const MAX_NAME_CHARS: usize = 40;
const MAX_TEMPLATE_CHARS: usize = 500;
const MAX_KEY_CHARS: usize = 512;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredEngine {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub api_key: String,
    /// Google Programmable Search engine id, used with its key.
    #[serde(default)]
    pub cx: String,
    /// Custom engines: the URL with `%s`.
    #[serde(default)]
    pub template: String,
    /// Custom engines: the header that carries `api_key` (for example `Authorization`).
    #[serde(default)]
    pub auth_header: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Stored {
    #[serde(default)]
    pub engines: Vec<StoredEngine>,
}

pub fn is_builtin(id: &str) -> bool {
    BUILTIN.iter().any(|(builtin, _, _)| *builtin == id)
}

impl Stored {
    /// The list a user starts with, and what "no saved settings" means.
    pub fn defaults() -> Self {
        Stored {
            engines: BUILTIN
                .iter()
                .map(|(id, name, on)| StoredEngine {
                    id: (*id).to_owned(),
                    name: (*name).to_owned(),
                    enabled: *on,
                    ..Default::default()
                })
                .collect(),
        }
    }

    /// Parse the saved value. Anything unreadable is treated as "nothing saved", and a built-in
    /// engine missing from the list (a newer version added one) is appended, off.
    pub fn parse(text: &str) -> Self {
        let mut stored: Stored = serde_json::from_str(text).unwrap_or_default();
        if stored.engines.is_empty() {
            return Stored::defaults();
        }
        for (id, name, _) in BUILTIN {
            if !stored.engines.iter().any(|e| e.id == id) {
                stored.engines.push(StoredEngine {
                    id: id.to_owned(),
                    name: name.to_owned(),
                    ..Default::default()
                });
            }
        }
        stored
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_owned())
    }

    /// What the interface may see: everything except the keys.
    pub fn public_view(&self) -> Value {
        let engines: Vec<Value> = self
            .engines
            .iter()
            .map(|e| {
                let builtin = is_builtin(&e.id);
                json!({
                    "id": e.id,
                    "name": if e.name.is_empty() { &e.id } else { &e.name },
                    "builtin": builtin,
                    "enabled": e.enabled,
                    // Brave and Tavily cannot work without a key; Google can, with or without.
                    "needs_key": matches!(e.id.as_str(), "brave" | "tavily"),
                    "accepts_key": !matches!(e.id.as_str(), "bing" | "duckduckgo"),
                    "has_key": !e.api_key.is_empty(),
                    "has_cx": !e.cx.is_empty(),
                    "template": e.template,
                    "auth_header": e.auth_header,
                })
            })
            .collect();
        json!({"ok": true, "engines": engines})
    }

    /// The engines to ask, in order: the ones that are on and have what they need.
    pub fn engines(&self) -> Vec<Engine> {
        self.engines
            .iter()
            .filter(|e| e.enabled)
            .filter_map(engine_of)
            .collect()
    }

    pub fn engine(&self, id: &str) -> Option<Engine> {
        self.engines.iter().find(|e| e.id == id).and_then(engine_of)
    }
}

fn engine_of(e: &StoredEngine) -> Option<Engine> {
    let name = if e.name.is_empty() {
        e.id.clone()
    } else {
        e.name.clone()
    };
    let key = (!e.api_key.is_empty()).then(|| e.api_key.clone());
    let kind = match e.id.as_str() {
        "google" => EngineKind::Google(key.zip((!e.cx.is_empty()).then(|| e.cx.clone()))),
        "bing" => EngineKind::Bing,
        "duckduckgo" => EngineKind::DuckDuckGo,
        "brave" => EngineKind::Brave(key?),
        "tavily" => EngineKind::Tavily(key?),
        id if id.starts_with("custom-") && e.template.contains("%s") => EngineKind::Custom {
            template: e.template.clone(),
            header: (!e.auth_header.is_empty())
                .then(|| key.map(|k| (e.auth_header.clone(), k)))
                .flatten(),
        },
        _ => return None,
    };
    Some(Engine {
        id: e.id.clone(),
        name,
        kind,
    })
}

/// One engine as the interface sends it back. A missing `api_key` keeps the saved one; an empty
/// string does too; `clear_key` removes it.
#[derive(Debug, Default, Deserialize)]
pub struct Incoming {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub clear_key: bool,
    #[serde(default)]
    pub cx: Option<String>,
    #[serde(default)]
    pub template: String,
    #[serde(default)]
    pub auth_header: String,
}

fn clean_secret(value: &str, what: &str) -> Result<String, String> {
    let value = value.trim();
    if value.chars().count() > MAX_KEY_CHARS || value.chars().any(char::is_control) {
        return Err(format!("that does not look like a valid {what}"));
    }
    Ok(value.to_owned())
}

fn valid_header_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 60
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Check the list the interface sent and merge it with what is saved. Built-in engines cannot be
/// removed (only turned off); custom ones can. Keys are kept unless replaced or cleared.
pub fn merge(existing: &Stored, incoming: Vec<Incoming>) -> Result<Stored, String> {
    if incoming.len() > MAX_ENGINES {
        return Err(format!("at most {MAX_ENGINES} search engines"));
    }
    let mut out: Vec<StoredEngine> = Vec::new();
    let mut next_custom = existing
        .engines
        .iter()
        .filter_map(|e| e.id.strip_prefix("custom-")?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    for item in incoming {
        let builtin = BUILTIN.iter().find(|(id, _, _)| *id == item.id);
        let saved = existing
            .engines
            .iter()
            .find(|e| e.id == item.id && !item.id.is_empty());
        let mut engine = StoredEngine {
            enabled: item.enabled,
            ..Default::default()
        };
        if let Some((id, name, _)) = builtin {
            engine.id = (*id).to_owned();
            engine.name = (*name).to_owned();
        } else {
            let name = item.name.trim();
            if name.is_empty()
                || name.chars().count() > MAX_NAME_CHARS
                || name.chars().any(char::is_control)
            {
                return Err("a search engine needs a name of up to 40 characters".to_owned());
            }
            let template = item.template.trim();
            if template.chars().count() > MAX_TEMPLATE_CHARS
                || template.chars().any(char::is_control)
                || !(template.starts_with("https://") || template.starts_with("http://"))
                || template.matches("%s").count() != 1
            {
                return Err(format!("{name}: the address must start with http:// or https:// and contain %s once, where the search words go"));
            }
            if !item.auth_header.is_empty() && !valid_header_name(item.auth_header.trim()) {
                return Err(format!(
                    "{name}: the header name may only use letters, digits, - and _"
                ));
            }
            engine.id = match saved {
                Some(saved) if saved.id.starts_with("custom-") => saved.id.clone(),
                _ => {
                    next_custom += 1;
                    format!("custom-{next_custom}")
                }
            };
            engine.name = name.to_owned();
            engine.template = template.to_owned();
            engine.auth_header = item.auth_header.trim().to_owned();
        }
        if out.iter().any(|e| e.id == engine.id) {
            return Err("a search engine appears twice".to_owned());
        }
        // Keys: keep what was saved unless replaced or cleared.
        engine.api_key = saved.map(|s| s.api_key.clone()).unwrap_or_default();
        engine.cx = saved.map(|s| s.cx.clone()).unwrap_or_default();
        if item.clear_key {
            engine.api_key.clear();
            engine.cx.clear();
        }
        if let Some(key) = item
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty())
        {
            engine.api_key = clean_secret(key, "API key")?;
        }
        if let Some(cx) = item.cx.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
            engine.cx = clean_secret(cx, "search engine id")?;
        }
        if engine.id == "google" && engine.api_key.is_empty() != engine.cx.is_empty() {
            return Err("Google's API needs both a key and a search engine id (or neither, to use the free search)".to_owned());
        }
        if matches!(engine.id.as_str(), "brave" | "tavily")
            && engine.enabled
            && engine.api_key.is_empty()
        {
            return Err(format!(
                "{} needs an API key before it can be turned on",
                engine.name
            ));
        }
        if !is_builtin(&engine.id) && engine.api_key.is_empty() {
            engine.auth_header.clear();
        }
        out.push(engine);
    }
    // A built-in engine the list left out is still there, off, at the end.
    for (id, name, _) in BUILTIN {
        if !out.iter().any(|e| e.id == id) {
            let saved = existing.engines.iter().find(|e| e.id == id).cloned();
            out.push(StoredEngine {
                id: id.to_owned(),
                name: name.to_owned(),
                enabled: false,
                api_key: saved
                    .as_ref()
                    .map(|s| s.api_key.clone())
                    .unwrap_or_default(),
                cx: saved.map(|s| s.cx).unwrap_or_default(),
                ..Default::default()
            });
        }
    }
    Ok(Stored { engines: out })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builtin_incoming() -> Vec<Incoming> {
        Stored::defaults()
            .engines
            .into_iter()
            .map(|e| Incoming {
                id: e.id,
                enabled: e.enabled,
                ..Default::default()
            })
            .collect()
    }

    fn custom(name: &str, template: &str) -> Incoming {
        Incoming {
            name: name.into(),
            enabled: true,
            template: template.into(),
            ..Default::default()
        }
    }

    #[test]
    fn defaults_are_google_bing_duckduckgo_on_and_the_keyed_services_off() {
        let engines = Stored::defaults().engines();
        let names: Vec<_> = engines.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Google", "Bing", "DuckDuckGo"]);
        assert_eq!(engines[0].kind, EngineKind::Google(None));
        // Unreadable or empty saved text is the same as nothing saved.
        assert_eq!(Stored::parse("not json"), Stored::defaults());
        assert_eq!(Stored::parse("{}"), Stored::defaults());
    }

    #[test]
    fn google_uses_the_api_only_with_both_key_and_engine_id() {
        let mut stored = Stored::defaults();
        stored.engines[0].api_key = "k".into();
        assert_eq!(stored.engines()[0].kind, EngineKind::Google(None));
        stored.engines[0].cx = "cx1".into();
        assert_eq!(
            stored.engines()[0].kind,
            EngineKind::Google(Some(("k".into(), "cx1".into())))
        );
    }

    #[test]
    fn a_custom_engine_is_a_name_and_a_url_with_one_percent_s() {
        let mut incoming = builtin_incoming();
        incoming.push(custom(
            "My SearXNG",
            "https://searx.example/search?q=%s&format=json",
        ));
        let stored = merge(&Stored::defaults(), incoming).unwrap();
        let mine = stored.engines.iter().find(|e| e.id == "custom-1").unwrap();
        assert_eq!(mine.name, "My SearXNG");
        assert_eq!(stored.engines().last().unwrap().name, "My SearXNG");
        for bad in [
            "searx.example/?q=%s",
            "ftp://x/?q=%s",
            "https://x/?q=",
            "https://x/?q=%s&r=%s",
            "javascript:alert(1)%s",
        ] {
            let mut incoming = builtin_incoming();
            incoming.push(custom("Bad", bad));
            assert!(merge(&Stored::defaults(), incoming).is_err(), "{bad}");
        }
        let mut incoming = builtin_incoming();
        incoming.push(custom("  ", "https://x/?q=%s"));
        assert!(merge(&Stored::defaults(), incoming)
            .unwrap_err()
            .contains("needs a name"));
    }

    #[test]
    fn custom_engines_keep_their_id_when_saved_again_and_ids_are_not_reused() {
        let mut incoming = builtin_incoming();
        incoming.push(custom("A", "https://a.example/?q=%s"));
        incoming.push(custom("B", "https://b.example/?q=%s"));
        let first = merge(&Stored::defaults(), incoming).unwrap();
        // The interface sends back the ids it was given; B is removed, C is added.
        let mut again = builtin_incoming();
        again.push(Incoming {
            id: "custom-1".into(),
            ..custom("A renamed", "https://a.example/?q=%s")
        });
        again.push(custom("C", "https://c.example/?q=%s"));
        let second = merge(&first, again).unwrap();
        let ids: Vec<_> = second
            .engines
            .iter()
            .filter(|e| e.id.starts_with("custom-"))
            .map(|e| e.id.as_str())
            .collect();
        assert_eq!(ids, ["custom-1", "custom-3"]);
        assert_eq!(
            second
                .engines
                .iter()
                .find(|e| e.id == "custom-1")
                .unwrap()
                .name,
            "A renamed"
        );
    }

    #[test]
    fn keys_are_kept_unless_replaced_or_cleared_and_never_shown() {
        let mut incoming = builtin_incoming();
        incoming
            .iter_mut()
            .find(|e| e.id == "brave")
            .unwrap()
            .api_key = Some(" BSA-1 ".into());
        incoming
            .iter_mut()
            .find(|e| e.id == "brave")
            .unwrap()
            .enabled = true;
        let saved = merge(&Stored::defaults(), incoming).unwrap();
        assert_eq!(
            saved
                .engines
                .iter()
                .find(|e| e.id == "brave")
                .unwrap()
                .api_key,
            "BSA-1"
        );
        assert!(!saved.public_view().to_string().contains("BSA-1"));
        assert_eq!(
            saved
                .engines()
                .iter()
                .filter(|e| e.name == "Brave Search")
                .count(),
            1
        );

        // Sending nothing, or an empty string, keeps it.
        let mut keep = builtin_incoming();
        let brave = keep.iter_mut().find(|e| e.id == "brave").unwrap();
        brave.enabled = true;
        brave.api_key = Some(String::new());
        assert_eq!(
            merge(&saved, keep)
                .unwrap()
                .engines
                .iter()
                .find(|e| e.id == "brave")
                .unwrap()
                .api_key,
            "BSA-1"
        );

        // Clearing it while the engine stays on is refused; clearing and turning it off is fine.
        let mut clear = builtin_incoming();
        let brave = clear.iter_mut().find(|e| e.id == "brave").unwrap();
        brave.enabled = true;
        brave.clear_key = true;
        assert!(merge(&saved, clear)
            .unwrap_err()
            .contains("needs an API key"));
        let mut off = builtin_incoming();
        off.iter_mut().find(|e| e.id == "brave").unwrap().clear_key = true;
        assert_eq!(
            merge(&saved, off)
                .unwrap()
                .engines
                .iter()
                .find(|e| e.id == "brave")
                .unwrap()
                .api_key,
            ""
        );

        let mut bad = builtin_incoming();
        bad.iter_mut().find(|e| e.id == "tavily").unwrap().api_key = Some("a\nb".into());
        assert!(merge(&saved, bad).is_err());
    }

    #[test]
    fn google_needs_a_key_and_an_engine_id_together() {
        let mut half = builtin_incoming();
        half.iter_mut().find(|e| e.id == "google").unwrap().api_key = Some("k".into());
        assert!(merge(&Stored::defaults(), half)
            .unwrap_err()
            .contains("both"));
        let mut both = builtin_incoming();
        let google = both.iter_mut().find(|e| e.id == "google").unwrap();
        google.api_key = Some("k".into());
        google.cx = Some("cx".into());
        assert!(merge(&Stored::defaults(), both).is_ok());
    }

    #[test]
    fn engines_can_be_reordered_and_turned_off_and_builtins_cannot_be_deleted() {
        let mut order = builtin_incoming();
        order.reverse();
        order.iter_mut().find(|e| e.id == "google").unwrap().enabled = false;
        let stored = merge(&Stored::defaults(), order).unwrap();
        let names: Vec<_> = stored.engines().iter().map(|e| e.name.clone()).collect();
        assert_eq!(names, ["DuckDuckGo", "Bing"]);
        // Leaving a built-in out of the list does not delete it.
        let mut fewer = builtin_incoming();
        fewer.retain(|e| e.id != "bing");
        let stored = merge(&Stored::defaults(), fewer).unwrap();
        assert!(stored.engines.iter().any(|e| e.id == "bing" && !e.enabled));
    }

    #[test]
    fn an_auth_header_is_only_sent_with_a_key() {
        let mut incoming = builtin_incoming();
        incoming.push(Incoming {
            auth_header: "X-API-Key".into(),
            api_key: Some("secret".into()),
            ..custom("Mine", "https://m.example/?q=%s")
        });
        let stored = merge(&Stored::defaults(), incoming).unwrap();
        match &stored.engines().last().unwrap().kind {
            EngineKind::Custom { header, .. } => {
                assert_eq!(header.as_ref().unwrap().0, "X-API-Key")
            }
            other => panic!("{other:?}"),
        }
        let mut bad = builtin_incoming();
        bad.push(Incoming {
            auth_header: "Bad Header".into(),
            ..custom("Mine", "https://m.example/?q=%s")
        });
        assert!(merge(&Stored::defaults(), bad).is_err());
    }
}
