//! Web search: ask a search engine and read titles, links and snippets out of what it returns.
//!
//! Engines are tried in the user's order and the first that yields results wins, so one being
//! blocked, unreachable or changing its markup does not end the search. An engine that fails is
//! skipped for a while so it does not slow every later search. Built-in engines read the public
//! result pages (Google, Bing, DuckDuckGo) or, with a key, the service's API (Google, Brave,
//! Tavily). A user-added engine is just a URL with `%s` for the query: whatever comes back is read
//! as JSON when it is JSON and as a web page otherwise.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde_json::{json, Value};
use url::Url;

use super::fetch::{decode_body, request_raw, Limits, Request};
use super::html::{attribute, elements, html_to_text, inline_text};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    /// When the page was published or last seen, as the engine words it ("2 days ago",
    /// "2026-10-20"), when the engine says.
    pub date: Option<String>,
}

#[derive(Debug)]
pub struct SearchOutput {
    /// Display name of the engine that answered.
    pub provider: String,
    pub results: Vec<SearchResult>,
    /// Why engines before the one that answered were skipped, when any were.
    pub note: Option<String>,
}

/// How an engine is asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineKind {
    /// The public result page, or Programmable Search with `(api key, search engine id)`.
    Google(Option<(String, String)>),
    Bing,
    DuckDuckGo,
    Brave(String),
    Tavily(String),
    /// A user-added URL with `%s`, and an optional `(header name, value)` such as an API key.
    Custom {
        template: String,
        header: Option<(String, String)>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Engine {
    pub id: String,
    pub name: String,
    pub kind: EngineKind,
}

/// How recent the results must be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recency {
    Day,
    Week,
    Month,
    Year,
}

impl Recency {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "day" | "d" | "24h" => Some(Self::Day),
            "week" | "w" => Some(Self::Week),
            "month" | "m" => Some(Self::Month),
            "year" | "y" => Some(Self::Year),
            _ => None,
        }
    }

    fn word(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
            Self::Year => "year",
        }
    }

    fn letter(self) -> char {
        self.word().as_bytes()[0] as char
    }
}

/// What to narrow a search to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filters {
    /// Only this site (a domain such as `docs.rs`).
    pub site: Option<String>,
    pub recency: Option<Recency>,
}

impl Filters {
    /// A bare domain from whatever the caller wrote (`https://www.docs.rs/x` -> `www.docs.rs`).
    pub fn site_from(text: &str) -> Option<String> {
        let text = text.trim();
        let host = Url::parse(text)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .unwrap_or_else(|| text.split('/').next().unwrap_or_default().to_owned());
        let host = host.trim_start_matches("site:").to_ascii_lowercase();
        let valid = !host.is_empty()
            && host.contains('.')
            && host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
        valid.then_some(host)
    }
}

/// Most results a caller may ask for.
pub const MAX_RESULTS: usize = 10;
pub const DEFAULT_RESULTS: usize = 5;
/// How long an engine that failed is skipped.
const FAILURE_COOLDOWN: Duration = Duration::from_secs(10 * 60);
/// How long an engine that answered with nothing is skipped (a block page looks like that).
const EMPTY_COOLDOWN: Duration = Duration::from_secs(2 * 60);

fn cooldowns() -> &'static Mutex<HashMap<String, Instant>> {
    static COOLDOWNS: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    COOLDOWNS.get_or_init(Default::default)
}

fn cooling(id: &str) -> bool {
    cooldowns()
        .lock()
        .ok()
        .and_then(|map| map.get(id).copied())
        .is_some_and(|until| Instant::now() < until)
}

fn bench(id: &str, how_long: Duration) {
    cooldowns()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(id.to_owned(), Instant::now() + how_long);
}

fn clear_bench(id: &str) {
    cooldowns()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(id);
}

fn json_results(
    value: &Value,
    list: &[&str],
    title: &str,
    url: &str,
    snippet: &str,
    date: &[&str],
) -> Vec<SearchResult> {
    let mut node = value;
    for part in list {
        node = node.get(part).unwrap_or(&Value::Null);
    }
    node.as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let text = |key: &str| item.get(key).and_then(Value::as_str).unwrap_or("");
                    let link = text(url);
                    let valid =
                        Url::parse(link).is_ok_and(|u| matches!(u.scheme(), "http" | "https"));
                    valid.then(|| SearchResult {
                        date: date
                            .iter()
                            .map(|key| text(key))
                            .find(|value| !value.trim().is_empty())
                            .map(|value| value.trim().to_owned()),
                        title: inline_text(text(title)),
                        url: link.to_owned(),
                        snippet: inline_text(text(snippet)),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Brave Search API: `{"web": {"results": [{"title", "url", "description"}]}}`.
pub fn parse_brave(body: &str) -> Vec<SearchResult> {
    serde_json::from_str::<Value>(body)
        .map(|v| {
            json_results(
                &v,
                &["web", "results"],
                "title",
                "url",
                "description",
                &["age", "page_age"],
            )
        })
        .unwrap_or_default()
}

/// Tavily: `{"results": [{"title", "url", "content"}]}`.
pub fn parse_tavily(body: &str) -> Vec<SearchResult> {
    serde_json::from_str::<Value>(body)
        .map(|v| {
            json_results(
                &v,
                &["results"],
                "title",
                "url",
                "content",
                &["published_date"],
            )
        })
        .unwrap_or_default()
}

const URL_KEYS: [&str; 4] = ["url", "link", "href", "source_url"];
const TITLE_KEYS: [&str; 3] = ["title", "name", "headline"];
const DATE_KEYS: [&str; 5] = ["published_date", "date", "published", "age", "page_age"];
const SNIPPET_KEYS: [&str; 6] = [
    "content",
    "snippet",
    "description",
    "body",
    "summary",
    "text",
];

fn first_string<'a>(item: &'a Value, keys: &[&str]) -> &'a str {
    keys.iter()
        .find_map(|key| {
            item.get(key)
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
        })
        .unwrap_or("")
}

/// Read search results out of any JSON answer: the first list (searched breadth first, a few levels
/// deep) of objects that each have a web address and a title. Covers SearXNG (`results`), Google
/// Programmable Search (`items`), Bocha-style (`data.webPages.value`) and most others.
pub fn parse_generic_json(body: &str) -> Vec<SearchResult> {
    let Ok(root) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    let mut level = vec![&root];
    for _ in 0..5 {
        let mut next = Vec::new();
        for node in level {
            match node {
                Value::Array(items) => {
                    let results: Vec<SearchResult> = items
                        .iter()
                        .filter_map(|item| {
                            let link = first_string(item, &URL_KEYS);
                            let title = first_string(item, &TITLE_KEYS);
                            let valid = Url::parse(link)
                                .is_ok_and(|u| matches!(u.scheme(), "http" | "https"));
                            (valid && !title.is_empty()).then(|| SearchResult {
                                date: Some(first_string(item, &DATE_KEYS))
                                    .filter(|date| !date.is_empty())
                                    .map(str::to_owned),
                                title: inline_text(title),
                                url: link.to_owned(),
                                snippet: inline_text(first_string(item, &SNIPPET_KEYS)),
                            })
                        })
                        .collect();
                    if !results.is_empty() {
                        return results;
                    }
                    next.extend(items.iter());
                }
                Value::Object(map) => next.extend(map.values()),
                _ => {}
            }
        }
        level = next;
    }
    Vec::new()
}

/// A redirect wrapper's real target: `/url?q=https://...`, `?url=`, `?u=`, `?uddg=` and so on.
fn unwrap_redirect(href: &Url, own_host: &str) -> Option<String> {
    let same_host = href
        .host_str()
        .is_some_and(|h| h == own_host || h.ends_with(&format!(".{own_host}")));
    if !same_host {
        return matches!(href.scheme(), "http" | "https").then(|| href.to_string());
    }
    href.query_pairs()
        .filter(|(key, _)| {
            matches!(
                key.as_ref(),
                "q" | "url" | "u" | "uddg" | "target" | "redirect"
            )
        })
        .find_map(|(_, value)| {
            Url::parse(&value)
                .ok()
                .filter(|u| matches!(u.scheme(), "http" | "https"))
                .map(|u| u.to_string())
        })
}

/// Text between two result anchors, as a short snippet: the title line is dropped, at most three
/// lines are kept.
fn snippet_between(page: &str, from: usize, to: usize, title: &str) -> String {
    let slice = page.get(from..to.min(page.len())).unwrap_or("");
    let lines: Vec<String> = html_to_text(slice)
        .lines()
        .map(str::trim)
        .filter(|l| l.chars().count() >= 12 && *l != title)
        .take(3)
        .map(str::to_owned)
        .collect();
    lines.join(" ").chars().take(240).collect()
}

/// Read results out of any search page: every link that leaves the engine's own site and has a
/// readable title. Best effort; it cannot recover results that are only reachable through opaque
/// tracking links.
pub fn parse_generic_html(html: &str, own_host: &str) -> Vec<SearchResult> {
    let anchors = elements(html, "a");
    let mut picked: Vec<(usize, usize, SearchResult)> = Vec::new();
    for anchor in &anchors {
        let Some(href) = attribute(anchor.open_tag, "href") else {
            continue;
        };
        let Ok(base) = Url::parse(&format!("https://{own_host}/")) else {
            continue;
        };
        let Ok(url) = base.join(&href) else {
            continue;
        };
        let Some(target) = unwrap_redirect(&url, own_host) else {
            continue;
        };
        // A heading inside the link is the title; otherwise the link text itself.
        let title = elements(anchor.inner, "h3")
            .first()
            .map(|h| inline_text(h.inner))
            .unwrap_or_else(|| inline_text(anchor.inner));
        if title.chars().count() < 8 || picked.iter().any(|(_, _, r)| r.url == target) {
            continue;
        }
        let end = anchor.start + anchor.open_tag.len() + anchor.inner.len();
        picked.push((
            anchor.start,
            end,
            SearchResult {
                date: None,
                title,
                url: target,
                snippet: String::new(),
            },
        ));
    }
    let starts: Vec<usize> = picked.iter().map(|(start, _, _)| *start).collect();
    picked
        .into_iter()
        .enumerate()
        .map(|(i, (_, end, mut result))| {
            let next = starts.get(i + 1).copied().unwrap_or(html.len());
            result.snippet = snippet_between(html, end, next, &result.title);
            result
        })
        .collect()
}

/// Google's result page: each result is a link with an `<h3>` title, either direct or wrapped as
/// `/url?q=<target>`.
pub fn parse_google(html: &str) -> Vec<SearchResult> {
    parse_generic_html(html, "www.google.com")
        .into_iter()
        .filter(|r| !r.url.contains("google.com/") || r.url.contains("/url?"))
        .collect()
}

fn has_class(open_tag: &str, class: &str) -> bool {
    attribute(open_tag, "class").is_some_and(|value| value.split_whitespace().any(|c| c == class))
}

/// DuckDuckGo wraps every result link in a redirect (`/l/?uddg=<encoded target>`).
fn duckduckgo_target(href: &str) -> Option<String> {
    let absolute = if let Some(rest) = href.strip_prefix("//") {
        format!("https://{rest}")
    } else {
        href.to_owned()
    };
    let url = Url::parse(&absolute).ok()?;
    if let Some((_, target)) = url.query_pairs().find(|(key, _)| key == "uddg") {
        return Some(target.into_owned());
    }
    matches!(url.scheme(), "http" | "https")
        .then(|| {
            url.host_str()
                .is_some_and(|host| !host.ends_with("duckduckgo.com"))
        })
        .and_then(|external| external.then_some(absolute))
}

pub fn parse_duckduckgo(html: &str) -> Vec<SearchResult> {
    let mut results: Vec<SearchResult> = Vec::new();
    let mut parts: Vec<(usize, bool, String, String)> = Vec::new();
    for tag in ["a", "div", "td"] {
        for element in elements(html, tag) {
            if has_class(element.open_tag, "result__a") {
                let href = attribute(element.open_tag, "href").unwrap_or_default();
                parts.push((element.start, true, href, inline_text(element.inner)));
            } else if has_class(element.open_tag, "result__snippet")
                || has_class(element.open_tag, "result-snippet")
            {
                parts.push((
                    element.start,
                    false,
                    String::new(),
                    inline_text(element.inner),
                ));
            }
        }
    }
    parts.sort_by_key(|part| part.0);
    for (_, is_link, href, text) in parts {
        if is_link {
            if let Some(url) = duckduckgo_target(&href) {
                results.push(SearchResult {
                    date: None,
                    title: text,
                    url,
                    snippet: String::new(),
                });
            }
        } else if let Some(last) = results.last_mut() {
            if last.snippet.is_empty() {
                last.snippet = text;
            }
        }
    }
    results
}

/// Bing links are often `bing.com/ck/a?...&u=a1<base64url of the target>`.
fn bing_target(href: &str) -> Option<String> {
    let url = Url::parse(href).ok()?;
    if url
        .host_str()
        .is_some_and(|host| host.ends_with("bing.com"))
    {
        if url.path().starts_with("/ck/a") {
            let (_, value) = url.query_pairs().find(|(key, _)| key == "u")?;
            let encoded = value.strip_prefix("a1")?;
            let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(encoded.trim_end_matches('='))
                .ok()?;
            let target = String::from_utf8(bytes).ok()?;
            return Url::parse(&target)
                .ok()
                .filter(|url| matches!(url.scheme(), "http" | "https"))
                .map(|_| target);
        }
        return None;
    }
    matches!(url.scheme(), "http" | "https").then(|| href.to_owned())
}

pub fn parse_bing(html: &str) -> Vec<SearchResult> {
    let mut results = Vec::new();
    for item in elements(html, "li") {
        if !has_class(item.open_tag, "b_algo") {
            continue;
        }
        let Some(heading) = elements(item.inner, "h2").into_iter().next() else {
            continue;
        };
        let Some(link) = elements(heading.inner, "a").into_iter().next() else {
            continue;
        };
        let Some(url) = attribute(link.open_tag, "href").and_then(|href| bing_target(&href)) else {
            continue;
        };
        let snippet = elements(item.inner, "p")
            .into_iter()
            .map(|p| inline_text(p.inner))
            .find(|text| !text.is_empty())
            .unwrap_or_default();
        results.push(SearchResult {
            date: None,
            title: inline_text(link.inner),
            url,
            snippet,
        });
    }
    results
}

/// Query parameters that only say where a visitor came from.
fn is_tracking_parameter(name: &str) -> bool {
    name.starts_with("utm_")
        || matches!(
            name,
            "fbclid"
                | "gclid"
                | "msclkid"
                | "yclid"
                | "igshid"
                | "mc_cid"
                | "mc_eid"
                | "mkt_tok"
                | "_hsenc"
                | "_hsmi"
                | "spm"
        )
}

/// The address without tracking parameters and without a `#fragment`: the same page reached by
/// two campaign links is one result.
pub fn clean_url(url: &str) -> String {
    let Ok(mut parsed) = Url::parse(url) else {
        return url.to_owned();
    };
    parsed.set_fragment(None);
    let all = parsed.query_pairs().count();
    let kept: Vec<(String, String)> = parsed
        .query_pairs()
        .filter(|(name, _)| !is_tracking_parameter(name))
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect();
    if kept.len() != all {
        if kept.is_empty() {
            parsed.set_query(None);
        } else {
            parsed.query_pairs_mut().clear().extend_pairs(kept);
        }
    }
    parsed.to_string()
}

/// What makes two addresses the same page for a reader: the host without `www.`, the path
/// without a trailing slash, the query.
fn same_page_key(url: &str) -> String {
    let Ok(parsed) = Url::parse(url) else {
        return url.to_owned();
    };
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    format!(
        "{}{}?{}",
        host.strip_prefix("www.").unwrap_or(&host),
        parsed.path().trim_end_matches('/'),
        parsed.query().unwrap_or_default()
    )
}

fn keep_valid(results: Vec<SearchResult>, max: usize) -> Vec<SearchResult> {
    let mut kept: Vec<SearchResult> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for mut result in results {
        if result.title.is_empty() {
            continue;
        }
        result.url = clean_url(&result.url);
        let key = same_page_key(&result.url);
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        kept.push(result);
        if kept.len() == max {
            break;
        }
    }
    kept
}

fn encode(query: &str) -> String {
    url::form_urlencoded::byte_serialize(query.as_bytes()).collect()
}

/// Hide anything after `?` in the request address wherever an error repeats it: some services
/// take their key in the query string.
fn scrub(message: String, url: &str) -> String {
    match url.split_once('?') {
        Some((base, _)) => message.replace(url, base),
        None => message,
    }
}

fn host_of(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Engines that cannot take `recency`, so the answer may include older pages.
fn ignores_recency(kind: &EngineKind, recency: Recency) -> bool {
    match kind {
        EngineKind::Custom { .. } => true,
        // Bing's page has fixed windows for a day, a week and a month only.
        EngineKind::Bing => recency == Recency::Year,
        _ => false,
    }
}

/// The request an engine is asked with: address, `Accept`, extra headers and an optional JSON body.
type Built<'a> = (String, &'static str, Vec<(&'a str, String)>, Option<String>);

fn build_request<'a>(engine: &'a Engine, query: &str, max: usize, filters: &Filters) -> Built<'a> {
    // Tavily takes the site as a field; every other engine reads the `site:` operator.
    let operator_query;
    let query = match (&filters.site, &engine.kind) {
        (Some(site), kind) if !matches!(kind, EngineKind::Tavily(_)) => {
            operator_query = format!("{query} site:{site}");
            operator_query.as_str()
        }
        _ => query,
    };
    let q = encode(query);
    let recency = filters.recency;
    let html_accept = "text/html,application/xhtml+xml";
    match &engine.kind {
        EngineKind::Google(Some((key, cx))) => (
            format!(
                "https://www.googleapis.com/customsearch/v1?key={}&cx={}&q={q}&num={}{}",
                encode(key),
                encode(cx),
                max.min(10),
                recency.map_or(String::new(), |r| format!("&dateRestrict={}1", r.letter()))
            ),
            "application/json",
            Vec::new(),
            None,
        ),
        EngineKind::Google(None) => (
            format!(
                "https://www.google.com/search?q={q}&num={max}&udm=14{}",
                recency.map_or(String::new(), |r| format!("&tbs=qdr:{}", r.letter()))
            ),
            html_accept,
            Vec::new(),
            None,
        ),
        EngineKind::Bing => (
            format!(
                "https://www.bing.com/search?q={q}&count={max}&ensearch=1{}",
                match recency {
                    Some(Recency::Day) => "&filters=ex1%3a%22ez1%22",
                    Some(Recency::Week) => "&filters=ex1%3a%22ez2%22",
                    Some(Recency::Month) => "&filters=ex1%3a%22ez3%22",
                    _ => "",
                }
            ),
            html_accept,
            Vec::new(),
            None,
        ),
        EngineKind::DuckDuckGo => (
            format!(
                "https://html.duckduckgo.com/html/?q={q}{}",
                recency.map_or(String::new(), |r| format!("&df={}", r.letter()))
            ),
            html_accept,
            Vec::new(),
            None,
        ),
        EngineKind::Brave(key) => (
            format!(
                "https://api.search.brave.com/res/v1/web/search?q={q}&count={max}{}",
                recency.map_or(String::new(), |r| format!("&freshness=p{}", r.letter()))
            ),
            "application/json",
            vec![("X-Subscription-Token", key.clone())],
            None,
        ),
        EngineKind::Tavily(key) => (
            "https://api.tavily.com/search".to_owned(),
            "application/json",
            vec![("Authorization", format!("Bearer {key}"))],
            Some({
                let mut body = json!({"query": query, "max_results": max, "search_depth": "basic"});
                if let Some(recency) = recency {
                    body["time_range"] = json!(recency.word());
                }
                if let Some(site) = &filters.site {
                    body["include_domains"] = json!([site]);
                }
                body.to_string()
            }),
        ),
        EngineKind::Custom { template, header } => (
            template.replacen("%s", &q, 1),
            "application/json,text/html;q=0.9,*/*;q=0.5",
            header
                .iter()
                .map(|(name, value)| (name.as_str(), value.clone()))
                .collect(),
            None,
        ),
    }
}

fn query_engine(
    engine: &Engine,
    query: &str,
    max: usize,
    filters: &Filters,
    limits: Limits,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<Vec<SearchResult>, String> {
    let (url, accept, headers, body) = build_request(engine, query, max, filters);
    let request = Request {
        url: &url,
        accept,
        headers,
        json_body: body,
    };
    let page = request_raw(&request, limits, is_cancelled).map_err(|error| {
        if error.contains("HTTP 401") || error.contains("HTTP 403") {
            format!(
                "{} refused the request ({error}); check its key in Settings",
                engine.name
            )
        } else {
            scrub(error, &url)
        }
    })?;
    let text = decode_body(&page.bytes, &page.content_type);
    Ok(match &engine.kind {
        EngineKind::Google(Some(_)) => parse_generic_json(&text),
        EngineKind::Google(None) => parse_google(&text),
        EngineKind::Bing => parse_bing(&text),
        EngineKind::DuckDuckGo => parse_duckduckgo(&text),
        EngineKind::Brave(_) => parse_brave(&text),
        EngineKind::Tavily(_) => parse_tavily(&text),
        EngineKind::Custom { template, .. } => {
            let looks_like_json = page.content_type.to_ascii_lowercase().contains("json")
                || text.trim_start().starts_with(['{', '[']);
            if looks_like_json {
                parse_generic_json(&text)
            } else {
                parse_generic_html(&text, &host_of(template))
            }
        }
    })
}

fn clamp_results(max_results: Option<i64>) -> usize {
    match max_results {
        Some(n) if n > 0 => (n as usize).min(MAX_RESULTS),
        _ => DEFAULT_RESULTS,
    }
}

/// Put the engine the user asked for (by id or name, any case) first, keeping the rest in the
/// user's order as the fallback. `None` in the second place means nothing matched, so the
/// caller can say so instead of silently using the default.
pub fn prefer(engines: &[Engine], wanted: &str) -> (Vec<Engine>, bool) {
    let wanted = wanted.trim();
    let Some(at) = engines
        .iter()
        .position(|e| e.id.eq_ignore_ascii_case(wanted) || e.name.eq_ignore_ascii_case(wanted))
    else {
        return (engines.to_vec(), false);
    };
    let mut ordered = engines.to_vec();
    let chosen = ordered.remove(at);
    ordered.insert(0, chosen);
    (ordered, true)
}

/// Ask one engine only, ignoring any cooldown: what the Settings "Test" button runs.
pub fn search_one(
    engine: &Engine,
    query: &str,
    limits: Limits,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<Vec<SearchResult>, String> {
    let results = keep_valid(
        query_engine(
            engine,
            query.trim(),
            DEFAULT_RESULTS,
            &Filters::default(),
            limits,
            is_cancelled,
        )?,
        DEFAULT_RESULTS,
    );
    if results.is_empty() {
        return Err("the engine answered, but no results could be read from the answer".to_owned());
    }
    Ok(results)
}

/// Search with the user's engines, in order, until one yields results.
pub fn search(
    query: &str,
    max_results: Option<i64>,
    engines: &[Engine],
    limits: Limits,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<SearchOutput, String> {
    search_with(
        query,
        max_results,
        &Filters::default(),
        engines,
        limits,
        is_cancelled,
    )
}

/// Search as [`search`] does, narrowed to a site and a time window when `filters` say so.
pub fn search_with(
    query: &str,
    max_results: Option<i64>,
    filters: &Filters,
    engines: &[Engine],
    limits: Limits,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<SearchOutput, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("query is required".to_owned());
    }
    if engines.is_empty() {
        return Err("no search engine is turned on; turn one on in Settings".to_owned());
    }
    let max = clamp_results(max_results);
    // Skip engines that failed a moment ago, unless that would leave nothing to ask.
    let rested: Vec<&Engine> = engines.iter().filter(|e| !cooling(&e.id)).collect();
    let order: Vec<&Engine> = if rested.is_empty() {
        engines.iter().collect()
    } else {
        rested
    };
    let mut problems = Vec::new();
    for engine in order {
        // Every engine in the fallback chain leaves by the same route: clone the
        // limits rather than moving them, so the second engine is not locked out
        // by the first one's attempt.
        match query_engine(engine, query, max, filters, limits.clone(), is_cancelled) {
            Ok(results) => {
                let results = keep_valid(results, max);
                if !results.is_empty() {
                    clear_bench(&engine.id);
                    if let Some(recency) = filters
                        .recency
                        .filter(|r| ignores_recency(&engine.kind, *r))
                    {
                        problems.push(format!(
                            "{} cannot limit results to the last {}, so older pages may be included",
                            engine.name,
                            recency.word()
                        ));
                    }
                    return Ok(SearchOutput {
                        provider: engine.name.clone(),
                        results,
                        note: (!problems.is_empty()).then(|| problems.join("; ")),
                    });
                }
                bench(&engine.id, EMPTY_COOLDOWN);
                problems.push(format!("{}: no results found on the page", engine.name));
            }
            Err(error) if error == "cancelled" => return Err(error),
            Err(error) => {
                bench(&engine.id, FAILURE_COOLDOWN);
                problems.push(format!("{}: {error}", engine.name));
            }
        }
    }
    Err(format!(
        "web search found nothing ({})",
        problems.join("; ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Hand-written samples in the shape these pages have; the live pages are not reachable from
    // the test environment, so the parsers are checked against the markup, not against the sites.
    const DDG: &str = r#"
      <div class="result results_links results_links_deep web-result">
        <h2 class="result__title"><a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.rust-lang.org%2F&amp;rut=abc">Rust <b>Programming</b> Language</a></h2>
        <a class="result__snippet" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.rust-lang.org%2F&amp;rut=abc">A language empowering <b>everyone</b> to build reliable software.</a>
      </div>
      <div class="result results_links results_links_deep web-result">
        <h2 class="result__title"><a rel="nofollow" class="result__a" href="https://doc.rust-lang.org/book/">The Rust Book</a></h2>
        <a class="result__snippet" href="https://doc.rust-lang.org/book/">Learn Rust &amp; ownership.</a>
      </div>
      <div class="result result--ad"><a class="result__a" href="https://duckduckgo.com/y.js?ad=1">Ad</a></div>
      <div class="result"><a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.rust-lang.org%2F">Duplicate</a></div>
    "#;

    fn bing_link(target: &str) -> String {
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(target);
        format!("https://www.bing.com/ck/a?!&amp;&amp;p=abc&amp;u=a1{encoded}&amp;ntb=1")
    }

    fn bing_sample() -> String {
        format!(
            r#"<ol id="b_results">
            <li class="b_algo"><h2><a href="{}" h="ID=SERP,5000.1">Rust <strong>language</strong></a></h2>
              <div class="b_caption"><p class="b_lineclamp2">Rust is fast &amp; safe.</p></div></li>
            <li class="b_algo"><h2><a href="https://github.com/rust-lang/rust">rust-lang/rust</a></h2>
              <div class="b_caption"><p>The Rust compiler.</p></div></li>
            <li class="b_ans"><h2><a href="https://example.com/">Not a result</a></h2></li>
            </ol>"#,
            bing_link("https://www.rust-lang.org/")
        )
    }

    #[test]
    fn duckduckgo_results_are_unwrapped_and_snippets_attached() {
        let results = parse_duckduckgo(DDG);
        assert_eq!(results[0].title, "Rust Programming Language");
        assert_eq!(results[0].url, "https://www.rust-lang.org/");
        assert_eq!(
            results[0].snippet,
            "A language empowering everyone to build reliable software."
        );
        assert_eq!(results[1].url, "https://doc.rust-lang.org/book/");
        assert_eq!(results[1].snippet, "Learn Rust & ownership.");
        // The advertisement points back at duckduckgo.com and is dropped; the duplicate is
        // dropped by `keep_valid`.
        assert_eq!(keep_valid(results, 10).len(), 2);
    }

    #[test]
    fn bing_results_are_unwrapped_and_other_blocks_ignored() {
        let results = parse_bing(&bing_sample());
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Rust language");
        assert_eq!(results[0].url, "https://www.rust-lang.org/");
        assert_eq!(results[0].snippet, "Rust is fast & safe.");
        assert_eq!(results[1].url, "https://github.com/rust-lang/rust");
    }

    fn engine(kind: EngineKind) -> Engine {
        Engine {
            id: "e".to_owned(),
            name: "E".to_owned(),
            kind,
        }
    }

    #[test]
    fn tracking_parameters_and_fragments_are_dropped_and_the_same_page_is_listed_once() {
        assert_eq!(
            clean_url("https://e.test/a?utm_source=x&id=7&fbclid=y#top"),
            "https://e.test/a?id=7"
        );
        assert_eq!(
            clean_url("https://e.test/a?utm_medium=m"),
            "https://e.test/a"
        );
        let result = |url: &str| SearchResult {
            title: "t".to_owned(),
            url: url.to_owned(),
            snippet: String::new(),
            date: None,
        };
        let kept = keep_valid(
            vec![
                result("https://www.e.test/post/?utm_campaign=a"),
                result("https://e.test/post#comments"),
                result("https://e.test/other"),
            ],
            10,
        );
        let urls: Vec<&str> = kept.iter().map(|r| r.url.as_str()).collect();
        assert_eq!(urls, ["https://www.e.test/post/", "https://e.test/other"]);
    }

    #[test]
    fn a_site_is_read_from_a_domain_or_an_address_and_nothing_else_is_accepted() {
        assert_eq!(Filters::site_from("docs.rs").as_deref(), Some("docs.rs"));
        assert_eq!(
            Filters::site_from("https://Arxiv.org/abs/1").as_deref(),
            Some("arxiv.org")
        );
        assert_eq!(
            Filters::site_from("site:docs.rs").as_deref(),
            Some("docs.rs")
        );
        assert_eq!(Filters::site_from("rust lang"), None);
        assert_eq!(Filters::site_from("localhost"), None);
        assert_eq!(Recency::parse("Week"), Some(Recency::Week));
        assert_eq!(Recency::parse("fortnight"), None);
    }

    #[test]
    fn each_engine_gets_the_site_and_the_time_window_in_its_own_way() {
        let filters = Filters {
            site: Some("docs.rs".to_owned()),
            recency: Some(Recency::Week),
        };
        let url = |kind| build_request(&engine(kind), "serde json", 5, &filters).0;
        assert!(url(EngineKind::Google(None)).contains("q=serde+json+site%3Adocs.rs"));
        assert!(url(EngineKind::Google(None)).contains("&tbs=qdr:w"));
        assert!(url(EngineKind::DuckDuckGo).contains("&df=w"));
        assert!(url(EngineKind::Bing).contains("filters=ex1%3a%22ez2%22"));
        let brave = url(EngineKind::Brave("k".to_owned()));
        assert!(
            brave.contains("site%3Adocs.rs") && brave.contains("&freshness=pw"),
            "{brave}"
        );
        let cse = url(EngineKind::Google(Some(("k".to_owned(), "cx".to_owned()))));
        assert!(cse.contains("&dateRestrict=w1"), "{cse}");

        // Tavily takes both as fields, and the query stays as written.
        let (_, _, _, body) = build_request(
            &engine(EngineKind::Tavily("k".to_owned())),
            "serde json",
            5,
            &filters,
        );
        let body: Value = serde_json::from_str(&body.unwrap()).unwrap();
        assert_eq!(body["query"], "serde json");
        assert_eq!(body["time_range"], "week");
        assert_eq!(body["include_domains"], json!(["docs.rs"]));

        // Without filters nothing is added.
        let plain = build_request(
            &engine(EngineKind::DuckDuckGo),
            "serde",
            5,
            &Filters::default(),
        )
        .0;
        assert_eq!(plain, "https://html.duckduckgo.com/html/?q=serde");
    }

    #[test]
    fn the_age_of_a_result_comes_through_when_the_engine_gives_it() {
        let brave = parse_brave(
            r#"{"web":{"results":[{"title":"A post","url":"https://e.test/a","description":"d","age":"2 days ago"}]}}"#,
        );
        assert_eq!(brave[0].date.as_deref(), Some("2 days ago"));
        let tavily = parse_tavily(
            r#"{"results":[{"title":"News","url":"https://e.test/n","content":"c","published_date":"2026-10-20"}]}"#,
        );
        assert_eq!(tavily[0].date.as_deref(), Some("2026-10-20"));
        let generic = parse_generic_json(
            r#"{"results":[{"title":"X","url":"https://e.test/x","content":"c","publishedDate":"ignored","date":"2026-01-02"}]}"#,
        );
        assert_eq!(generic[0].date.as_deref(), Some("2026-01-02"));
        assert!(parse_duckduckgo(DDG)[0].date.is_none());
    }

    #[test]
    fn a_page_with_no_results_yields_none_and_the_cap_is_applied() {
        assert!(parse_duckduckgo("<html><body>captcha</body></html>").is_empty());
        assert!(parse_bing("<html></html>").is_empty());
        let many: Vec<SearchResult> = (0..20)
            .map(|i| SearchResult {
                date: None,
                title: format!("t{i}"),
                url: format!("https://e.test/{i}"),
                snippet: String::new(),
            })
            .collect();
        assert_eq!(keep_valid(many, 3).len(), 3);
    }

    #[test]
    fn bing_redirect_links_that_do_not_decode_to_a_web_address_are_dropped() {
        assert_eq!(
            bing_target("https://www.bing.com/ck/a?u=a1bm90IGEgdXJs"),
            None
        );
        assert_eq!(bing_target("https://www.bing.com/images/search?q=x"), None);
        let file = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("file:///etc/passwd");
        assert_eq!(
            bing_target(&format!("https://www.bing.com/ck/a?u=a1{file}")),
            None
        );
    }

    #[test]
    fn keyed_service_answers_are_read_and_unusable_entries_dropped() {
        let brave = r#"{"web":{"results":[
            {"title":"Rust <strong>lang</strong>","url":"https://www.rust-lang.org/","description":"Fast &amp; safe"},
            {"title":"bad scheme","url":"javascript:alert(1)","description":"x"},
            {"title":"no url","description":"x"}]}}"#;
        let found = parse_brave(brave);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "Rust lang");
        assert_eq!(found[0].snippet, "Fast & safe");
        let tavily = r#"{"results":[{"title":"Docs","url":"https://doc.rust-lang.org/","content":"The book."}]}"#;
        assert_eq!(parse_tavily(tavily)[0].url, "https://doc.rust-lang.org/");
        assert!(parse_brave("not json").is_empty());
        assert!(parse_tavily("{}").is_empty());
    }

    const LOCAL: Limits = Limits {
        policy: crate::web::guard::AddressPolicy {
            allow_loopback: true,
        },
        ..Limits::PRODUCT
    };

    fn custom(id: &str, base: &str, path: &str) -> Engine {
        Engine {
            id: id.to_owned(),
            name: id.to_owned(),
            kind: EngineKind::Custom {
                template: format!("{base}{path}?q=%s"),
                header: None,
            },
        }
    }

    const SEARX: &str = r#"{"query":"rust","results":[
        {"url":"https://www.rust-lang.org/","title":"Rust","content":"A language"},
        {"url":"https://doc.rust-lang.org/","title":"Docs","content":"The book"}]}"#;

    #[test]
    fn any_json_answer_with_titled_links_is_read() {
        let found = parse_generic_json(SEARX);
        assert_eq!(found.len(), 2);
        assert_eq!(found[1].snippet, "The book");
        // Google Programmable Search.
        let google =
            r#"{"items":[{"title":"Rust","link":"https://www.rust-lang.org/","snippet":"Fast"}]}"#;
        assert_eq!(
            parse_generic_json(google)[0].url,
            "https://www.rust-lang.org/"
        );
        // Deeper nesting, other key names.
        let nested = r#"{"data":{"webPages":{"value":[{"name":"Rust","url":"https://www.rust-lang.org/","summary":"Fast"}]}}}"#;
        assert_eq!(parse_generic_json(nested)[0].title, "Rust");
        assert!(parse_generic_json(r#"{"results":[{"title":"no link"}]}"#).is_empty());
        assert!(parse_generic_json("<html>").is_empty());
    }

    #[test]
    fn any_results_page_is_read_by_its_outbound_links_and_unwrapped_redirects() {
        let page = r#"<html><body>
          <a href="/">Home page of the engine</a>
          <a href="/settings">Settings and preferences</a>
          <div><a href="https://www.rust-lang.org/"><h3>Rust Programming Language</h3></a>
            <p>A language empowering everyone to build reliable and efficient software.</p></div>
          <div><a href="/url?q=https%3A%2F%2Fdoc.rust-lang.org%2Fbook%2F&amp;sa=U">The Rust Book online</a>
            <span>Learn Rust with the official book, chapter by chapter.</span></div>
          <a href="https://www.rust-lang.org/">Rust Programming Language (again)</a>
          <a href="https://x.test/">short</a>
        </body></html>"#;
        let found = parse_generic_html(page, "search.example");
        let urls: Vec<_> = found.iter().map(|r| r.url.as_str()).collect();
        assert_eq!(
            urls,
            [
                "https://www.rust-lang.org/",
                "https://doc.rust-lang.org/book/"
            ]
        );
        assert_eq!(found[0].title, "Rust Programming Language");
        assert!(
            found[0].snippet.contains("empowering everyone"),
            "{}",
            found[0].snippet
        );
        assert!(found[1].snippet.contains("official book"));
    }

    #[test]
    fn google_result_links_are_unwrapped_and_its_own_links_ignored() {
        let page = r#"<div><a href="/url?q=https://www.rust-lang.org/&amp;sa=U&amp;ved=x"><h3>Rust Programming Language</h3><div>www.rust-lang.org</div></a>
            <div>A language empowering everyone to build reliable and efficient software.</div></div>
          <a href="https://accounts.google.com/ServiceLogin">Sign in to your account</a>
          <a href="https://policies.google.com/privacy">Privacy and terms of use</a>
          <a href="https://en.wikipedia.org/wiki/Rust_(programming_language)"><h3>Rust (programming language) - Wikipedia</h3></a>"#;
        let found = parse_google(page);
        let urls: Vec<_> = found.iter().map(|r| r.url.as_str()).collect();
        assert_eq!(
            urls,
            [
                "https://www.rust-lang.org/",
                "https://en.wikipedia.org/wiki/Rust_(programming_language)"
            ]
        );
    }

    #[test]
    fn engines_are_tried_in_order_a_failed_one_is_skipped_next_time_and_the_note_says_why() {
        use crate::web::fetch::test_server::{response, serve};
        let (broken, broken_server) = serve(1, |_| response("500 Server Error", &[], b"oops"));
        let (good, good_server) = serve(2, |_| {
            response(
                "200 OK",
                &[("Content-Type", "application/json")],
                SEARX.as_bytes(),
            )
        });
        let engines = [
            custom("order-broken", &broken, "/s"),
            custom("order-good", &good, "/s"),
        ];
        let first = search("rust", None, &engines, LOCAL, &|| false).unwrap();
        assert_eq!(first.provider, "order-good");
        assert_eq!(first.results.len(), 2);
        let note = first.note.unwrap();
        assert!(
            note.contains("order-broken") && note.contains("HTTP 500"),
            "{note}"
        );
        // Second search: the broken engine is resting, so it is not asked and not mentioned.
        let second = search("rust", Some(1), &engines, LOCAL, &|| false).unwrap();
        assert_eq!(second.results.len(), 1);
        assert!(second.note.is_none());
        broken_server.join().unwrap();
        good_server.join().unwrap();
    }

    #[test]
    fn a_page_without_readable_results_counts_as_empty_and_every_failure_is_reported() {
        use crate::web::fetch::test_server::{response, serve};
        let (base, server) = serve(1, |_| {
            response(
                "200 OK",
                &[("Content-Type", "text/html")],
                b"<html><body>Please verify you are a human</body></html>",
            )
        });
        let engines = [custom("empty-page", &base, "/s")];
        let error = search("rust", None, &engines, LOCAL, &|| false).unwrap_err();
        server.join().unwrap();
        assert!(error.contains("empty-page: no results"), "{error}");
        // One-engine test button ignores any cooldown and says what happened.
        let again = search_one(&engines[0], "rust", LOCAL, &|| false).unwrap_err();
        assert!(
            again.contains("connection")
                || again.contains("fetch failed")
                || again.contains("no results"),
            "{again}"
        );
    }

    #[test]
    fn an_error_never_repeats_a_key_that_travels_in_the_address() {
        let url = "https://www.googleapis.com/customsearch/v1?key=SECRET&cx=1&q=x";
        let message = format!("fetch failed: {url}: Connection Failed");
        let scrubbed = scrub(message, url);
        assert!(!scrubbed.contains("SECRET"), "{scrubbed}");
        assert!(scrubbed.contains("https://www.googleapis.com/customsearch/v1"));
    }

    #[test]
    fn a_named_engine_goes_first_and_an_unknown_name_changes_nothing() {
        let engine = |id: &str, name: &str| Engine {
            id: id.into(),
            name: name.into(),
            kind: EngineKind::Bing,
        };
        let list = [
            engine("google", "Google"),
            engine("bing", "Bing"),
            engine("custom-1", "My SearXNG"),
        ];
        let (ordered, found) = prefer(&list, " my searxng ");
        assert!(found);
        assert_eq!(
            ordered.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["custom-1", "google", "bing"]
        );
        let (same, found) = prefer(&list, "bing");
        assert!(found && same[0].id == "bing" && same.len() == 3);
        let (unchanged, found) = prefer(&list, "yahoo");
        assert!(!found);
        assert_eq!(unchanged, list);
    }

    #[test]
    fn an_empty_query_is_refused_without_a_request() {
        let engine = Engine {
            id: "e".into(),
            name: "E".into(),
            kind: EngineKind::Bing,
        };
        let error = search("  ", None, &[engine], Limits::PRODUCT, &|| false).unwrap_err();
        assert_eq!(error, "query is required");
        assert!(search("x", None, &[], Limits::PRODUCT, &|| false)
            .unwrap_err()
            .contains("turn one on"));
    }
}
