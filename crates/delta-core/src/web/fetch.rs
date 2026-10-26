//! Fetching one page over HTTP(S) for the model.
//!
//! Every hop is vetted by the address guard and the connection is pinned to the address that
//! passed it (the name still goes into the Host header and the TLS handshake, so certificates are
//! checked against the name). Redirects are walked here, never by the HTTP library, so a public
//! URL cannot bounce the request to a private one. The body is read in pieces and stops at a size
//! and a time limit, so a server that never stops sending cannot exhaust memory.

use std::io::{self, Read};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use super::guard::{vet, AddressPolicy};
use super::html;
use super::markdown;

/// Most bytes read from the network, after decompression.
pub const MAX_BODY_BYTES: usize = 5 * 1024 * 1024;
/// Most time spent reading one page.
pub const READ_DEADLINE: Duration = Duration::from_secs(30);
const MAX_REDIRECTS: usize = 5;
/// Characters returned when the caller does not say.
pub const DEFAULT_MAX_CHARS: usize = 20_000;
/// Most characters a caller may ask for.
pub const HARD_MAX_CHARS: usize = 100_000;
/// Most bytes read for a document (PDF, Word, Excel, PowerPoint), which is bigger than a page.
pub const MAX_DOCUMENT_BYTES: usize = 25 * 1024 * 1024;
/// Many sites turn away an unknown agent, so Delta says it is a compatible browser and names
/// itself.
const USER_AGENT: &str = concat!(
    "Mozilla/5.0 (compatible; Delta/",
    env!("CARGO_PKG_VERSION"),
    "; +desktop)"
);
const ACCEPT_LANGUAGE: &str = "en-US,en;q=0.9,zh-CN;q=0.8,zh;q=0.7";

#[derive(Debug, Clone)]
pub struct Limits {
    pub max_bytes: usize,
    /// The most bytes of a document; a page is held to `max_bytes`.
    pub max_document_bytes: usize,
    pub deadline: Duration,
    pub policy: AddressPolicy,
    /// Which proxy, if any, an outgoing request may use. `None` means direct. Carried on
    /// the request rather than read from a global so a caller that has its own policy
    /// (the agent's own egress decisions, for instance) is never overridden by ambient
    /// configuration, and so tests can pin the behaviour.
    pub proxy: Option<ureq::Proxy>,
}

impl Limits {
    pub const PRODUCT: Limits = Limits {
        max_bytes: MAX_BODY_BYTES,
        max_document_bytes: MAX_DOCUMENT_BYTES,
        deadline: READ_DEADLINE,
        policy: AddressPolicy::STRICT,
        proxy: None,
    };

    /// The limits with a proxy policy applied. Used at the boundary where a request is
    /// handed to the web door: the proxy is resolved once per turn, not per hop.
    pub fn with_proxy(mut self, proxy: Option<ureq::Proxy>) -> Self {
        self.proxy = proxy;
        self
    }
}

/// What came back, before any interpretation.
#[derive(Debug)]
pub struct RawPage {
    /// The URL that finally answered (the name, not the pinned address).
    pub url: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
    /// The body was longer than the limits allow, or the deadline passed.
    pub cut_short: bool,
}

/// A readable page.
#[derive(Debug)]
pub struct Page {
    pub url: String,
    pub content_type: String,
    pub title: Option<String>,
    /// The page as Markdown (headings, lists, links, tables, code), or the text of a document.
    pub text: String,
    /// More of the page remains after `text`, or the page was cut short on the way in.
    pub truncated: bool,
    /// Characters in the whole readable page.
    pub total_chars: usize,
    /// Where to continue reading (`offset` of the next call), when `text` stops short.
    pub next_offset: Option<usize>,
    /// Links in the readable part of the page. Filled only when asked for.
    pub links: Vec<markdown::Link>,
    /// Something the reader should know, such as a page that needs scripts to show its content.
    pub note: Option<String>,
}

/// What to read from a page.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReadOptions {
    /// Cap on returned characters.
    pub max_chars: Option<i64>,
    /// Characters to skip from the start of the readable page, to continue a long one.
    pub offset: Option<i64>,
    /// Also return the page's links.
    pub links: bool,
}

/// Makes the connection use the addresses that passed the guard instead of resolving the name
/// again.
#[derive(Debug)]
struct Pinned(Vec<SocketAddr>);

impl ureq::Resolver for Pinned {
    fn resolve(&self, _netloc: &str) -> io::Result<Vec<SocketAddr>> {
        Ok(self.0.clone())
    }
}

/// GET `url`, following redirects through the guard.
pub fn fetch_raw(
    url: &str,
    accept: &str,
    limits: Limits,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<RawPage, String> {
    request_raw(&Request::get(url, accept), limits, is_cancelled)
}

/// One HTTP request. A request with a JSON body is a POST and is never redirected: an API that
/// answers a POST with a redirect is misconfigured, and the body (which may carry a key) must not
/// follow it anywhere.
pub struct Request<'a> {
    pub url: &'a str,
    pub accept: &'a str,
    /// Extra headers, such as an API key. Sent only to `url`, never on a redirect.
    pub headers: Vec<(&'a str, String)>,
    pub json_body: Option<String>,
}

impl<'a> Request<'a> {
    pub fn get(url: &'a str, accept: &'a str) -> Self {
        Request {
            url,
            accept,
            headers: Vec::new(),
            json_body: None,
        }
    }
}

pub fn request_raw(
    request: &Request<'_>,
    limits: Limits,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<RawPage, String> {
    let mut current = request.url.to_owned();
    let redirects = if request.json_body.is_some() {
        0
    } else {
        MAX_REDIRECTS
    };
    for hop in 0..=redirects {
        if is_cancelled() {
            return Err("cancelled".to_owned());
        }
        let target = vet(&current, limits.policy)?;
        let mut builder = ureq::AgentBuilder::new()
            .user_agent(USER_AGENT)
            .redirects(0)
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(Duration::from_secs(20))
            .resolver(Pinned(target.addrs.clone()));
        // The web door now follows the same proxy policy as every other exit. Leaving it
        // out is what produced "the browser can reach it but web_fetch cannot" — and that
        // asymmetry is what pushed the model towards the browser for plain searches.
        if let Some(proxy) = &limits.proxy {
            builder = builder.proxy(proxy.clone());
        }
        let agent = builder.build();
        let mut call = match &request.json_body {
            Some(_) => agent.post(target.url.as_str()),
            None => agent.get(target.url.as_str()),
        }
        .set("Accept", request.accept)
        .set("Accept-Language", ACCEPT_LANGUAGE);
        // Credentials go to the URL the caller named and nowhere a redirect might lead.
        if hop == 0 {
            for (name, value) in &request.headers {
                call = call.set(name, value);
            }
        }
        let result = match &request.json_body {
            Some(body) => call
                .set("Content-Type", "application/json")
                .send_string(body),
            None => call.call(),
        };
        let response = match result {
            Ok(response) => response,
            Err(ureq::Error::Status(code, _)) => {
                return Err(format!("the server answered HTTP {code}"))
            }
            Err(error) => return Err(format!("fetch failed: {error}")),
        };
        if (300..400).contains(&response.status()) {
            if request.json_body.is_some() {
                return Err("the server redirected an API request".to_owned());
            }
            if let Some(location) = response.header("location") {
                // A relative Location stays on the logical host, not on the pinned address.
                current = target
                    .url
                    .join(location)
                    .map_err(|_| "the server sent an invalid redirect".to_owned())?
                    .to_string();
                continue;
            }
        }
        let content_type = response
            .header("content-type")
            .unwrap_or_default()
            .to_owned();
        // A document is bigger than a page; the cap follows what the server says it is sending.
        let mut read_limits = limits.clone();
        if document_kind(&content_type, target.url.path()).is_some() {
            read_limits.max_bytes = limits.max_bytes.max(limits.max_document_bytes);
        }
        let (bytes, cut_short) = read_capped(response.into_reader(), read_limits, is_cancelled)?;
        return Ok(RawPage {
            url: target.url.to_string(),
            content_type,
            bytes,
            cut_short,
        });
    }
    Err(format!("too many redirects (more than {MAX_REDIRECTS})"))
}

/// Read until the body ends, `max_bytes` are held or the deadline passes. Never holds more than
/// `max_bytes`, whatever the server sends or however well the body compresses. A failure after
/// some data has arrived keeps that data and marks the page cut short.
fn read_capped(
    mut reader: impl Read,
    limits: Limits,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<(Vec<u8>, bool), String> {
    let deadline = Instant::now() + limits.deadline;
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        if is_cancelled() {
            return Err("cancelled".to_owned());
        }
        let room = limits.max_bytes - bytes.len();
        let want = buffer.len().min(room + 1);
        let count = match reader.read(&mut buffer[..want]) {
            Ok(0) => return Ok((bytes, false)),
            Ok(count) => count,
            Err(error) if bytes.is_empty() => return Err(format!("fetch failed: {error}")),
            Err(_) => return Ok((bytes, true)),
        };
        if count > room {
            bytes.extend_from_slice(&buffer[..room]);
            return Ok((bytes, true));
        }
        bytes.extend_from_slice(&buffer[..count]);
        if Instant::now() > deadline {
            return Ok((bytes, true));
        }
    }
}

fn charset_from_header(content_type: &str) -> Option<String> {
    let lower = content_type.to_ascii_lowercase();
    let (_, rest) = lower.split_once("charset=")?;
    let label: String = rest
        .trim_start_matches(['"', '\''])
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    (!label.is_empty()).then_some(label)
}

fn charset_from_html(head: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(head).to_ascii_lowercase();
    charset_from_header(&text)
}

/// Decode a page body: the declared charset (header, then `<meta>`), UTF-8 otherwise. Pages in
/// GBK or Big5 are common for Chinese sites, so this is not optional.
pub fn decode_body(bytes: &[u8], content_type: &str) -> String {
    let is_html = content_type.to_ascii_lowercase().contains("html");
    let label = charset_from_header(content_type).or_else(|| {
        is_html
            .then(|| charset_from_html(&bytes[..bytes.len().min(2048)]))
            .flatten()
    });
    let encoding = label
        .and_then(|label| encoding_rs::Encoding::for_label(label.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    encoding.decode(bytes).0.into_owned()
}

fn is_text_like(content_type: &str) -> bool {
    let ct = content_type.to_ascii_lowercase();
    ct.is_empty()
        || ct.starts_with("text/")
        || ct.contains("json")
        || ct.contains("xml")
        || ct.contains("javascript")
        || ct.contains("yaml")
}

/// Which document format a response is, from its content type and then its path.
fn document_kind(content_type: &str, path: &str) -> Option<crate::document::Kind> {
    use crate::document::Kind;
    let ct = content_type.to_ascii_lowercase();
    if ct.contains("application/pdf") {
        return Some(Kind::Pdf);
    }
    if ct.contains("wordprocessingml") {
        return Some(Kind::Docx);
    }
    if ct.contains("spreadsheetml") {
        return Some(Kind::Xlsx);
    }
    if ct.contains("presentationml") {
        return Some(Kind::Pptx);
    }
    // A server that sends a document as a plain download still names it in the path.
    if ct.is_empty() || ct.contains("octet-stream") {
        return crate::document::kind_of(std::path::Path::new(path)).ok();
    }
    None
}

/// Whether an error is worth one more try: the server is busy or briefly unavailable.
fn is_transient(error: &str) -> bool {
    ["HTTP 429", "HTTP 502", "HTTP 503", "HTTP 504"]
        .iter()
        .any(|code| error.contains(code))
        || error.contains("timed out")
}

/// The pause before the one retry of a busy server.
const RETRY_PAUSE: Duration = Duration::from_millis(1200);

fn get_with_retry(
    url: &str,
    limits: &Limits,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<RawPage, String> {
    let request = Request::get(
        url,
        "text/html,application/xhtml+xml,application/pdf;q=0.9,text/plain;q=0.9,*/*;q=0.5",
    );
    match request_raw(&request, limits.clone(), is_cancelled) {
        Err(error) if is_transient(&error) => {
            let until = Instant::now() + RETRY_PAUSE;
            while Instant::now() < until {
                if is_cancelled() {
                    return Err("cancelled".to_owned());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            request_raw(&request, limits.clone(), is_cancelled)
        }
        other => other,
    }
}

/// Fetch one page and return its readable text, at most `max_chars` characters.
pub fn fetch_page(
    url: &str,
    max_chars: Option<i64>,
    limits: Limits,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<Page, String> {
    fetch_page_with(
        url,
        ReadOptions {
            max_chars,
            ..ReadOptions::default()
        },
        limits,
        is_cancelled,
    )
}

/// Fetch one page or document and return it as readable text: a web page as Markdown, a PDF or
/// Office file as the text `read_document` would give. `options` picks the part to return.
pub fn fetch_page_with(
    url: &str,
    options: ReadOptions,
    limits: Limits,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<Page, String> {
    let cap = match options.max_chars {
        Some(n) if n > 0 => (n as usize).min(HARD_MAX_CHARS),
        _ => DEFAULT_MAX_CHARS,
    };
    let offset = options.offset.filter(|n| *n > 0).map_or(0, |n| n as usize);
    let raw = get_with_retry(url, &limits, is_cancelled)?;
    let final_url =
        url::Url::parse(&raw.url).map_err(|_| "the server sent an invalid address".to_owned())?;

    let mut title = None;
    let mut links = Vec::new();
    let mut note = None;
    let readable = if let Some(kind) = document_kind(&raw.content_type, final_url.path()) {
        if raw.cut_short {
            return Err(format!(
                "this document is larger than {} MB, so it was not read",
                limits.max_bytes.max(limits.max_document_bytes) / (1024 * 1024)
            ));
        }
        let document = crate::document::parse(kind, &raw.bytes)?;
        document
            .blocks
            .iter()
            .map(|block| format!("## {}\n\n{}", block.label, block.text.trim()))
            .collect::<Vec<_>>()
            .join("\n\n")
    } else if !is_text_like(&raw.content_type) {
        return Err(format!(
            "this is not a page or a document Delta can read (content type {})",
            raw.content_type
        ));
    } else {
        let body = decode_body(&raw.bytes, &raw.content_type);
        if raw.content_type.to_ascii_lowercase().contains("html") {
            title = html::title_of(&body);
            let extracted = markdown::html_to_markdown(&body, &final_url);
            if extracted.needs_javascript {
                note = Some(
                    "This page builds its content with scripts, so almost nothing was readable \
                     without them. If the content matters, open it with browser_navigate and \
                     read it with browser_snapshot."
                        .to_owned(),
                );
            }
            if options.links {
                links = extracted.links;
            }
            extracted.markdown
        } else {
            body
        }
    };

    let total_chars = readable.chars().count();
    let text: String = readable.chars().skip(offset).take(cap).collect();
    let end = offset + text.chars().count();
    let more = end < total_chars;
    Ok(Page {
        url: raw.url,
        content_type: raw.content_type,
        title,
        text,
        truncated: raw.cut_short || more,
        total_chars,
        next_offset: more.then_some(end),
        links,
        note,
    })
}

#[cfg(test)]
pub(super) mod test_server {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::thread;

    /// Serves `handler(path)` -> raw HTTP response bytes for `connections` requests.
    pub fn serve(
        connections: usize,
        handler: impl Fn(&str) -> Vec<u8> + Send + 'static,
    ) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            for _ in 0..connections {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request_line = String::new();
                let _ = reader.read_line(&mut request_line);
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                }
                let path = request_line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/")
                    .to_owned();
                let _ = stream.write_all(&handler(&path));
                let _ = stream.flush();
            }
        });
        (base, handle)
    }

    pub fn response(status: &str, headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
        let mut out = format!(
            "HTTP/1.1 {status}\r\nConnection: close\r\nContent-Length: {}\r\n",
            body.len()
        );
        for (name, value) in headers {
            out.push_str(&format!("{name}: {value}\r\n"));
        }
        out.push_str("\r\n");
        let mut bytes = out.into_bytes();
        bytes.extend_from_slice(body);
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::test_server::{response, serve};
    use super::*;

    const LOCAL: Limits = Limits {
        max_bytes: MAX_BODY_BYTES,
        max_document_bytes: MAX_DOCUMENT_BYTES,
        deadline: READ_DEADLINE,
        policy: AddressPolicy {
            allow_loopback: true,
        },
        proxy: None,
    };
    fn never() -> bool {
        false
    }

    #[test]
    fn a_page_comes_back_as_readable_text_with_its_title() {
        let (base, server) = serve(1, |_| {
            response(
                "200 OK",
                &[("Content-Type", "text/html; charset=utf-8")],
                "<html><head><title>Hi</title></head><body><p>Hello <b>world</b></p></body></html>"
                    .as_bytes(),
            )
        });
        let page = fetch_page(&format!("{base}/"), None, LOCAL, &never).unwrap();
        server.join().unwrap();
        assert_eq!(page.text, "Hello world");
        assert_eq!(page.title.as_deref(), Some("Hi"));
        assert!(!page.truncated);
    }

    #[test]
    fn relative_redirects_are_followed_on_the_same_host() {
        let (base, server) = serve(2, |path| match path {
            "/a" => response("302 Found", &[("Location", "/b")], b""),
            _ => response("200 OK", &[("Content-Type", "text/plain")], b"arrived"),
        });
        let page = fetch_page(&format!("{base}/a"), None, LOCAL, &never).unwrap();
        server.join().unwrap();
        assert_eq!(page.text, "arrived");
        assert!(page.url.ends_with("/b"));
    }

    #[test]
    fn a_redirect_to_a_private_address_is_refused_on_that_hop() {
        let (base, server) = serve(1, |_| {
            response(
                "302 Found",
                &[("Location", "http://169.254.169.254/latest/meta-data/")],
                b"",
            )
        });
        let error = fetch_page(&format!("{base}/"), None, LOCAL, &never).unwrap_err();
        server.join().unwrap();
        assert!(error.contains("refusing to fetch"), "{error}");
        assert!(error.contains("link-local"), "{error}");
    }

    #[test]
    fn redirect_loops_stop() {
        let (base, server) = serve(MAX_REDIRECTS + 1, |_| {
            response("302 Found", &[("Location", "/again")], b"")
        });
        let error = fetch_page(&format!("{base}/"), None, LOCAL, &never).unwrap_err();
        server.join().unwrap();
        assert!(error.contains("too many redirects"), "{error}");
    }

    #[test]
    fn the_body_is_capped_and_marked_truncated() {
        let (base, server) = serve(1, |_| {
            response(
                "200 OK",
                &[("Content-Type", "text/plain")],
                &vec![b'x'; 10_000],
            )
        });
        let limits = Limits {
            max_bytes: 1000,
            ..LOCAL
        };
        let raw = fetch_raw(&format!("{base}/"), "*/*", limits, &never).unwrap();
        server.join().unwrap();
        assert_eq!(raw.bytes.len(), 1000);
        assert!(raw.cut_short);
    }

    #[test]
    fn the_character_cap_applies_after_extraction() {
        let (base, server) = serve(1, |_| {
            response(
                "200 OK",
                &[("Content-Type", "text/plain")],
                "字".repeat(50).as_bytes(),
            )
        });
        let page = fetch_page(&format!("{base}/"), Some(10), LOCAL, &never).unwrap();
        server.join().unwrap();
        assert_eq!(page.text.chars().count(), 10);
        assert!(page.truncated);
    }

    #[test]
    fn http_errors_and_content_that_cannot_be_read_are_reported_plainly() {
        let (base, server) = serve(3, |path| match path {
            "/missing" => response("404 Not Found", &[], b"nope"),
            "/broken.pdf" => response(
                "200 OK",
                &[("Content-Type", "application/pdf")],
                b"%PDF-1.7",
            ),
            _ => response("200 OK", &[("Content-Type", "image/png")], b"\x89PNG"),
        });
        let missing = fetch_page(&format!("{base}/missing"), None, LOCAL, &never).unwrap_err();
        assert!(missing.contains("HTTP 404"), "{missing}");
        let broken = fetch_page(&format!("{base}/broken.pdf"), None, LOCAL, &never).unwrap_err();
        assert!(
            !broken.contains("not a page"),
            "a PDF is tried as a PDF: {broken}"
        );
        let image = fetch_page(&format!("{base}/pic"), None, LOCAL, &never).unwrap_err();
        server.join().unwrap();
        assert!(image.contains("not a page or a document"), "{image}");
        assert!(image.contains("image/png"), "{image}");
    }

    #[test]
    fn a_word_file_on_the_web_is_read_like_read_document_would() {
        let (_, docx, _) = crate::document::create(
            "report.docx",
            &serde_json::json!({"title": "Quarterly report", "blocks": [
                {"type": "paragraph", "text": "Revenue grew in the third quarter."}
            ]}),
        )
        .unwrap();
        let (base, server) = serve(1, move |_| {
            response(
                "200 OK",
                &[(
                    "Content-Type",
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                )],
                &docx,
            )
        });
        let page = fetch_page(&format!("{base}/files/report.docx"), None, LOCAL, &never).unwrap();
        server.join().unwrap();
        assert!(
            page.text.contains("Revenue grew in the third quarter."),
            "{}",
            page.text
        );
        assert!(page.title.is_none());
    }

    #[test]
    fn an_html_page_comes_back_as_markdown_with_links_and_a_place_to_continue() {
        let body = format!(
            "<html><head><title>Docs</title></head><body><nav>Menu</nav><main><h1>Guide</h1>\
             <p>{} See <a href=\"/next\">the next page</a>.</p></main></body></html>",
            "Words go here. ".repeat(40)
        );
        let (base, server) = serve(2, move |_| {
            response("200 OK", &[("Content-Type", "text/html")], body.as_bytes())
        });
        let options = ReadOptions {
            max_chars: Some(100),
            offset: None,
            links: true,
        };
        let first = fetch_page_with(&format!("{base}/"), options, LOCAL, &never).unwrap();
        assert!(
            first.text.starts_with("# Guide\n\nWords go here."),
            "{}",
            first.text
        );
        assert!(!first.text.contains("Menu"));
        assert!(first.truncated);
        assert_eq!(first.next_offset, Some(100));
        assert!(first.total_chars > 100);
        assert_eq!(first.links.len(), 1);
        assert_eq!(first.links[0].url, format!("{base}/next"));

        let second = fetch_page_with(
            &format!("{base}/"),
            ReadOptions {
                offset: first.next_offset.map(|n| n as i64),
                ..options
            },
            LOCAL,
            &never,
        )
        .unwrap();
        server.join().unwrap();
        let whole: String = first.text.chars().chain(second.text.chars()).collect();
        assert_eq!(
            whole.chars().count(),
            200,
            "the second page continues where the first stopped"
        );
        assert!(!second.text.starts_with("# Guide"));
    }

    #[test]
    fn a_busy_server_is_tried_once_more() {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = calls.clone();
        let (base, server) = serve(2, move |_| {
            if seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                response("503 Service Unavailable", &[], b"busy")
            } else {
                response("200 OK", &[("Content-Type", "text/plain")], b"ready")
            }
        });
        let page = fetch_page(&format!("{base}/"), None, LOCAL, &never).unwrap();
        server.join().unwrap();
        assert_eq!(page.text, "ready");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn a_page_that_scripts_build_says_so() {
        let (base, server) = serve(1, |_| {
            response(
                "200 OK",
                &[("Content-Type", "text/html")],
                br#"<html><body><div id="root"></div><script src="/app.js"></script></body></html>"#,
            )
        });
        let page = fetch_page(&format!("{base}/"), None, LOCAL, &never).unwrap();
        server.join().unwrap();
        assert!(page
            .note
            .as_deref()
            .unwrap_or("")
            .contains("browser_navigate"));
    }

    #[test]
    fn gbk_pages_decode_by_header_or_meta_tag() {
        // "中文" in GBK.
        let gbk = [0xD6, 0xD0, 0xCE, 0xC4];
        assert_eq!(decode_body(&gbk, "text/html; charset=gbk"), "中文");
        let mut with_meta = b"<html><head><meta charset=\"gb2312\"></head><body>".to_vec();
        with_meta.extend_from_slice(&gbk);
        assert!(decode_body(&with_meta, "text/html").contains("中文"));
        assert_eq!(decode_body("中文".as_bytes(), "text/html"), "中文");
    }

    #[test]
    fn strict_policy_refuses_the_local_server() {
        let error = fetch_page("http://127.0.0.1:9/", None, Limits::PRODUCT, &never).unwrap_err();
        assert!(error.contains("loopback"), "{error}");
    }

    #[test]
    fn cancellation_is_honoured_before_any_request() {
        let error = fetch_page("http://127.0.0.1:9/", None, LOCAL, &|| true).unwrap_err();
        assert_eq!(error, "cancelled");
    }
}
