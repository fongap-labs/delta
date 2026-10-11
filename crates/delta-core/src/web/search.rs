//! Web search without an API key: ask a public search page and read titles, links and snippets
//! out of the HTML it returns. Providers are tried in order and the first that yields results
//! wins, so one being unreachable or changing its markup does not end the search.
//!
//! This is the baseline. A keyed provider (a search API with a stable JSON answer) is the better
//! source and plugs in at the same place once Settings can hold its key.

use base64::Engine;
use url::Url;

use serde_json::{json, Value};

use super::fetch::{decode_body, fetch_raw, request_raw, Limits, Request};
use super::html::{attribute, elements, inline_text};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

#[derive(Debug)]
pub struct SearchOutput {
    pub provider: &'static str,
    pub results: Vec<SearchResult>,
    /// Why a better source was skipped (a rejected key, an unreachable service), when one was.
    pub note: Option<String>,
}

/// Search services with an API: stable JSON answers, better results, a key from the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyedProvider {
    Brave,
    Tavily,
}

impl KeyedProvider {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "brave" => Some(Self::Brave),
            "tavily" => Some(Self::Tavily),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Brave => "brave",
            Self::Tavily => "tavily",
        }
    }
}

/// A provider and the user's key for it.
#[derive(Clone, Copy)]
pub struct SearchKey<'a> {
    pub provider: KeyedProvider,
    pub key: &'a str,
}

fn json_results(
    value: &Value,
    list: &[&str],
    title: &str,
    url: &str,
    snippet: &str,
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
        .map(|v| json_results(&v, &["web", "results"], "title", "url", "description"))
        .unwrap_or_default()
}

/// Tavily: `{"results": [{"title", "url", "content"}]}`.
pub fn parse_tavily(body: &str) -> Vec<SearchResult> {
    serde_json::from_str::<Value>(body)
        .map(|v| json_results(&v, &["results"], "title", "url", "content"))
        .unwrap_or_default()
}

fn search_with_key(
    search_key: SearchKey<'_>,
    query: &str,
    max: usize,
    limits: Limits,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<Vec<SearchResult>, String> {
    let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
    let (url, headers, body): (String, Vec<(&str, String)>, Option<String>) = match search_key
        .provider
    {
        KeyedProvider::Brave => (
            format!("https://api.search.brave.com/res/v1/web/search?q={encoded}&count={max}"),
            vec![("X-Subscription-Token", search_key.key.to_owned())],
            None,
        ),
        KeyedProvider::Tavily => (
            "https://api.tavily.com/search".to_owned(),
            vec![("Authorization", format!("Bearer {}", search_key.key))],
            Some(json!({"query": query, "max_results": max, "search_depth": "basic"}).to_string()),
        ),
    };
    let request = Request {
        url: &url,
        accept: "application/json",
        headers,
        json_body: body,
    };
    let page = request_raw(&request, limits, is_cancelled).map_err(|error| {
        if error.contains("HTTP 401") || error.contains("HTTP 403") {
            format!(
                "{} rejected the search key ({error}); check it in Settings",
                search_key.provider.name()
            )
        } else {
            error
        }
    })?;
    let body = decode_body(&page.bytes, &page.content_type);
    Ok(match search_key.provider {
        KeyedProvider::Brave => parse_brave(&body),
        KeyedProvider::Tavily => parse_tavily(&body),
    })
}

/// Most results a caller may ask for.
pub const MAX_RESULTS: usize = 10;
pub const DEFAULT_RESULTS: usize = 5;

struct Provider {
    name: &'static str,
    endpoint: &'static str,
    parse: fn(&str) -> Vec<SearchResult>,
}

const PROVIDERS: [Provider; 2] = [
    Provider {
        name: "duckduckgo",
        endpoint: "https://html.duckduckgo.com/html/?q=",
        parse: parse_duckduckgo,
    },
    Provider {
        name: "bing",
        endpoint: "https://www.bing.com/search?q=",
        parse: parse_bing,
    },
];

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
            title: inline_text(link.inner),
            url,
            snippet,
        });
    }
    results
}

fn keep_valid(results: Vec<SearchResult>, max: usize) -> Vec<SearchResult> {
    let mut kept: Vec<SearchResult> = Vec::new();
    for result in results {
        if result.title.is_empty() || kept.iter().any(|seen| seen.url == result.url) {
            continue;
        }
        kept.push(result);
        if kept.len() == max {
            break;
        }
    }
    kept
}

pub fn search(
    query: &str,
    max_results: Option<i64>,
    key: Option<SearchKey<'_>>,
    limits: Limits,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<SearchOutput, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("query is required".to_owned());
    }
    let max = match max_results {
        Some(n) if n > 0 => (n as usize).min(MAX_RESULTS),
        _ => DEFAULT_RESULTS,
    };
    let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
    let mut problems = Vec::new();
    // The user's own search service first; if it fails, the keyless ones still answer, and the
    // note says why the better source was skipped.
    if let Some(search_key) = key {
        match search_with_key(search_key, query, max, limits, is_cancelled) {
            Ok(results) => {
                let results = keep_valid(results, max);
                if !results.is_empty() {
                    return Ok(SearchOutput {
                        provider: search_key.provider.name(),
                        results,
                        note: None,
                    });
                }
                problems.push(format!("{}: no results", search_key.provider.name()));
            }
            Err(error) if error == "cancelled" => return Err(error),
            Err(error) => problems.push(format!("{}: {error}", search_key.provider.name())),
        }
    }
    let skipped = (!problems.is_empty()).then(|| problems.join("; "));
    for provider in &PROVIDERS {
        let url = format!("{}{encoded}", provider.endpoint);
        match fetch_raw(
            &url,
            "text/html,application/xhtml+xml",
            limits,
            is_cancelled,
        ) {
            Ok(page) => {
                let body = decode_body(&page.bytes, &page.content_type);
                let results = keep_valid((provider.parse)(&body), max);
                if !results.is_empty() {
                    return Ok(SearchOutput {
                        provider: provider.name,
                        results,
                        note: skipped.clone(),
                    });
                }
                problems.push(format!("{}: no results found on the page", provider.name));
            }
            Err(error) if error == "cancelled" => return Err(error),
            Err(error) => problems.push(format!("{}: {error}", provider.name)),
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

    #[test]
    fn a_page_with_no_results_yields_none_and_the_cap_is_applied() {
        assert!(parse_duckduckgo("<html><body>captcha</body></html>").is_empty());
        assert!(parse_bing("<html></html>").is_empty());
        let many: Vec<SearchResult> = (0..20)
            .map(|i| SearchResult {
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
        assert_eq!(
            KeyedProvider::from_name("brave"),
            Some(KeyedProvider::Brave)
        );
        assert_eq!(KeyedProvider::from_name("google"), None);
    }

    #[test]
    fn an_empty_query_is_refused_without_a_request() {
        let error = search("  ", None, None, Limits::PRODUCT, &|| false).unwrap_err();
        assert_eq!(error, "query is required");
    }
}
