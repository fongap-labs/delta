//! Keyless public web-search provider.
//!
//! Use DuckDuckGo's HTML results and fall back to its lighter page. No API key,
//! Python runtime or third-party search subscription is needed. Both requests
//! use the same address guard, proxy, cancellation and body limits as web_fetch.

use url::Url;

use super::fetch::{decode_body, request_raw, Limits, Request};
use super::html::{attribute, elements, inline_text};
use super::search::{
    duckduckgo_target, keep_valid, parse_duckduckgo, Filters, Recency, SearchResult,
};

const HTML_ENDPOINT: &str = "https://html.duckduckgo.com/html/";
const LITE_ENDPOINT: &str = "https://lite.duckduckgo.com/lite/";

pub struct DuckDuckGoProvider;

impl DuckDuckGoProvider {
    pub fn search(
        query: &str,
        max: usize,
        filters: &Filters,
        limits: Limits,
        is_cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<SearchResult>, String> {
        let mut problems = Vec::new();
        for (endpoint, parser) in [
            (
                HTML_ENDPOINT,
                parse_duckduckgo as fn(&str) -> Vec<SearchResult>,
            ),
            (LITE_ENDPOINT, parse_lite as fn(&str) -> Vec<SearchResult>),
        ] {
            if is_cancelled() {
                return Err("cancelled".to_owned());
            }
            let url = request_url(endpoint, query, filters);
            match request_raw(
                &Request::get(&url, "text/html,application/xhtml+xml"),
                limits.clone(),
                is_cancelled,
            ) {
                Ok(page) => {
                    let html = decode_body(&page.bytes, &page.content_type);
                    let results = keep_valid(parser(&html), max);
                    if !results.is_empty() {
                        return Ok(results);
                    }
                    problems.push("no readable results".to_owned());
                }
                Err(error) if error == "cancelled" => return Err(error),
                Err(error) => problems.push(error),
            }
        }
        Err(format!(
            "DuckDuckGo HTML and Lite were unavailable: {}",
            problems.join("; ")
        ))
    }
}

fn request_url(endpoint: &str, query: &str, filters: &Filters) -> String {
    let mut url = Url::parse(endpoint).expect("built-in search endpoint must be valid");
    let query = if let Some(site) = &filters.site {
        format!("{query} site:{site}")
    } else {
        query.to_owned()
    };
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("q", &query);
        if let Some(recency) = filters.recency {
            let period = match recency {
                Recency::Day => "d",
                Recency::Week => "w",
                Recency::Month => "m",
                Recency::Year => "y",
            };
            pairs.append_pair("df", period);
        }
    }
    url.to_string()
}

/// Lite has result-link anchors and result-snippet cells rather than HTML's
/// result__a/result__snippet classes. Associate each snippet with its
/// preceding result link, and never follow DuckDuckGo's redirect addresses.
fn parse_lite(html: &str) -> Vec<SearchResult> {
    let mut parts: Vec<(usize, bool, String, String)> = Vec::new();
    for anchor in elements(html, "a") {
        if attribute(anchor.open_tag, "class")
            .is_some_and(|class| class.split_whitespace().any(|c| c == "result-link"))
        {
            let href = attribute(anchor.open_tag, "href").unwrap_or_default();
            parts.push((anchor.start, true, href, inline_text(anchor.inner)));
        }
    }
    for cell in elements(html, "td") {
        if attribute(cell.open_tag, "class")
            .is_some_and(|class| class.split_whitespace().any(|c| c == "result-snippet"))
        {
            parts.push((cell.start, false, String::new(), inline_text(cell.inner)));
        }
    }
    parts.sort_by_key(|part| part.0);
    let mut results: Vec<SearchResult> = Vec::new();
    for (_, is_link, href, content) in parts {
        if is_link {
            if let Some(url) = duckduckgo_target(&href) {
                results.push(SearchResult {
                    title: content,
                    url,
                    snippet: String::new(),
                    date: None,
                });
            }
        } else if let Some(last) = results.last_mut() {
            if last.snippet.is_empty() {
                last.snippet = content;
            }
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web::fetch::test_server::{response, serve};
    use crate::web::guard::AddressPolicy;

    #[test]
    fn both_keyless_endpoints_receive_the_same_filters() {
        let filters = Filters {
            site: Some("docs.rs".to_owned()),
            recency: Some(Recency::Week),
        };
        for endpoint in [HTML_ENDPOINT, LITE_ENDPOINT] {
            let url = request_url(endpoint, "rust async", &filters);
            let pairs: Vec<_> = Url::parse(&url)
                .unwrap()
                .query_pairs()
                .into_owned()
                .collect();
            assert!(pairs.contains(&("q".to_owned(), "rust async site:docs.rs".to_owned())));
            assert!(pairs.contains(&("df".to_owned(), "w".to_owned())));
            assert!(!url.contains("api_key"));
        }
    }

    #[test]
    fn lite_results_are_read_without_ads_and_with_snippets() {
        let html = r#"<table>
            <tr><td><a class="result-link" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fdocs.rs%2F">Rust documentation</a></td></tr>
            <tr><td class="result-snippet">Find <b>Rust</b> crates.</td></tr>
            <tr><td><a class="result-link" href="https://example.org/manual">An example manual</a></td></tr>
            <tr><td class="result-snippet">Example reference material.</td></tr>
            <tr><td><a href="https://ads.example/ad">Sponsored</a></td></tr>
        </table>"#;
        let results = keep_valid(parse_lite(html), 10);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].url, "https://docs.rs/");
        assert_eq!(results[0].snippet, "Find Rust crates.");
        assert_eq!(results[1].snippet, "Example reference material.");
    }

    #[test]
    fn keyless_request_uses_the_existing_address_guard() {
        let local = Limits {
            policy: AddressPolicy {
                allow_loopback: true,
            },
            ..Limits::PRODUCT
        };
        let (base, server) = serve(1, |_| {
            response("200 OK", &[("Content-Type", "text/html")], b"<html></html>")
        });
        let url = format!("{base}/test");
        let page = request_raw(&Request::get(&url, "text/html"), local, &|| false).unwrap();
        assert_eq!(page.content_type, "text/html");
        server.join().unwrap();
        assert!(request_raw(
            &Request::get("http://127.0.0.1:1/", "text/html"),
            Limits::PRODUCT,
            &|| false,
        )
        .is_err());
    }
}
