//! Just enough HTML handling to turn a page into readable text and to pick links and snippets out
//! of a search page. It is a forgiving scanner, not a parser: it never fails, and a page it cannot
//! make sense of yields little text rather than an error.

/// Decode the character references that matter in page text (`&amp;`, `&#39;`, `&#x4e2d;`, ...).
pub fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        // A reference is at most ~10 characters and ends with `;`.
        let end = rest.char_indices().take(12).find(|(_, c)| *c == ';');
        let decoded = end.and_then(|(end, _)| {
            let name = &rest[1..end];
            let value = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                "ndash" => Some('–'),
                "mdash" => Some('—'),
                "hellip" => Some('…'),
                "lsquo" => Some('‘'),
                "rsquo" => Some('’'),
                "ldquo" => Some('“'),
                "rdquo" => Some('”'),
                "copy" => Some('©'),
                _ => name.strip_prefix('#').and_then(|number| {
                    let code = match number.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok(),
                        None => number.parse::<u32>().ok(),
                    };
                    code.and_then(char::from_u32)
                }),
            };
            value.map(|c| (c, end + 1))
        });
        match decoded {
            Some((c, used)) => {
                out.push(c);
                rest = &rest[used..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Tags whose content is not page text.
const SKIPPED: [&str; 6] = ["script", "style", "noscript", "svg", "head", "template"];
/// Tags that end a line of text.
const BLOCKS: [&str; 24] = [
    "p",
    "div",
    "br",
    "li",
    "ul",
    "ol",
    "tr",
    "table",
    "section",
    "article",
    "header",
    "footer",
    "nav",
    "main",
    "aside",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "pre",
    "blockquote",
    "hr",
];

fn tag_name(tag: &str) -> (String, bool) {
    let inner = tag.trim_start_matches('<').trim_end_matches('>');
    let closing = inner.starts_with('/');
    let name: String = inner
        .trim_start_matches('/')
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    (name, closing)
}

/// The page title, if there is one.
pub fn title_of(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let open_end = lower[start..].find('>')? + start + 1;
    let close = lower[open_end..].find("</title")? + open_end;
    let title = collapse_whitespace(&decode_entities(&html[open_end..close]));
    (!title.is_empty()).then_some(title)
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Readable text of an HTML page: scripts, styles and the head are dropped, block tags become line
/// breaks and entities are decoded; blank lines are dropped.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 3);
    let mut skip_depth = 0usize;
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        let text = &rest[..lt];
        if skip_depth == 0 {
            out.push_str(&decode_entities(text));
        }
        let at_lt = &rest[lt..];
        if let Some(after) = at_lt.strip_prefix("<!--") {
            rest = after.find("-->").map_or("", |end| &after[end + 3..]);
            continue;
        }
        let Some(gt) = at_lt.find('>') else {
            // An unterminated tag: nothing readable follows.
            rest = "";
            break;
        };
        let tag = &at_lt[..=gt];
        let (name, closing) = tag_name(tag);
        if name.is_empty() {
            // `<` that was just a less-than sign: keep the text after it.
            if skip_depth == 0 {
                out.push('<');
            }
            rest = &at_lt[1..];
            continue;
        }
        rest = &at_lt[gt + 1..];
        if SKIPPED.contains(&name.as_str()) {
            if closing {
                skip_depth = skip_depth.saturating_sub(1);
            } else if !tag.ends_with("/>") {
                skip_depth += 1;
            }
            continue;
        }
        if skip_depth == 0 && BLOCKS.contains(&name.as_str()) {
            out.push('\n');
        }
    }
    if skip_depth == 0 {
        out.push_str(&decode_entities(rest));
    }
    // One line per block, no blank lines: compact text is cheaper for the model to read.
    let folded: Vec<String> = out
        .lines()
        .map(collapse_whitespace)
        .filter(|line| !line.is_empty())
        .collect();
    folded.join("\n").trim().to_owned()
}

/// Value of attribute `name` in an opening tag such as `<a class="x" href='y'>`.
pub fn attribute(open_tag: &str, name: &str) -> Option<String> {
    let lower = open_tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find(name) {
        let at = from + found;
        from = at + name.len();
        let before_ok = at > 0 && lower.as_bytes()[at - 1].is_ascii_whitespace();
        let after = open_tag[at + name.len()..].trim_start();
        if !before_ok || !after.starts_with('=') {
            continue;
        }
        let value = after[1..].trim_start();
        let (quote, body) = match value.chars().next() {
            Some(q @ ('"' | '\'')) => (Some(q), &value[1..]),
            _ => (None, value),
        };
        let end = match quote {
            Some(q) => body.find(q)?,
            None => body
                .find(|c: char| c.is_ascii_whitespace() || c == '>')
                .unwrap_or(body.len()),
        };
        return Some(decode_entities(&body[..end]));
    }
    None
}

/// One element found by [`elements`]: its opening tag and its inner HTML.
pub struct Element<'a> {
    pub open_tag: &'a str,
    pub inner: &'a str,
    /// Offset of the opening tag in the scanned text.
    pub start: usize,
}

/// Every `<name ...>inner</name>` in `html`, in order. Nesting of the same tag is not followed
/// (the first closing tag ends the element), which is right for the anchors and paragraphs this is
/// used for.
pub fn elements<'a>(html: &'a str, name: &str) -> Vec<Element<'a>> {
    let lower = html.to_ascii_lowercase();
    let open = format!("<{name}");
    let close = format!("</{name}");
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(rel) = lower[from..].find(&open) {
        let start = from + rel;
        let after_name = start + open.len();
        // `<a` must not match `<abbr`.
        if !lower[after_name..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_whitespace() || c == '>' || c == '/')
        {
            from = after_name;
            continue;
        }
        let Some(open_end_rel) = lower[start..].find('>') else {
            break;
        };
        let open_end = start + open_end_rel + 1;
        let (inner_end, next) = match lower[open_end..].find(&close) {
            Some(close_rel) => {
                let close_start = open_end + close_rel;
                let next = lower[close_start..]
                    .find('>')
                    .map_or(lower.len(), |gt| close_start + gt + 1);
                (close_start, next)
            }
            None => (lower.len(), lower.len()),
        };
        found.push(Element {
            open_tag: &html[start..open_end],
            inner: &html[open_end..inner_end],
            start,
        });
        from = next;
    }
    found
}

/// Text of an inner-HTML fragment on one line.
pub fn inline_text(fragment: &str) -> String {
    collapse_whitespace(&html_to_text(fragment))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entities_decode_named_and_numeric_forms() {
        assert_eq!(
            decode_entities("a &amp; b &lt;c&gt; &quot;d&quot; &#39;e&#39;"),
            "a & b <c> \"d\" 'e'"
        );
        assert_eq!(decode_entities("&#x4e2d;&#25991; &nbsp;x"), "中文  x");
        assert_eq!(
            decode_entities("AT&T and &unknown; &"),
            "AT&T and &unknown; &"
        );
    }

    #[test]
    fn page_text_drops_scripts_styles_and_head_and_keeps_line_breaks() {
        let html = "<html><head><title>T</title><style>p{color:red}</style></head>\
            <body><h1>Hello &amp; welcome</h1><script>var x = '<p>no</p>';</script>\
            <p>First   paragraph</p><!-- hidden --><p>Second<br>line</p>\
            <ul><li>one</li><li>two</li></ul></body></html>";
        assert_eq!(
            html_to_text(html),
            "Hello & welcome\nFirst paragraph\nSecond\nline\none\ntwo"
        );
        assert_eq!(title_of(html).as_deref(), Some("T"));
    }

    #[test]
    fn malformed_html_never_panics_and_keeps_what_is_readable() {
        assert_eq!(html_to_text("a < b and c > d"), "a < b and c > d");
        assert_eq!(html_to_text("<p>open"), "open");
        assert_eq!(html_to_text("text <b"), "text");
        assert_eq!(html_to_text("<script>never closed"), "");
        assert_eq!(html_to_text(""), "");
    }

    #[test]
    fn attributes_are_read_with_either_quote_or_none() {
        let tag = "<a class=\"result__a\" HREF='https://x.test/?a=1&amp;b=2' data-x=plain>";
        assert_eq!(attribute(tag, "class").as_deref(), Some("result__a"));
        assert_eq!(
            attribute(tag, "href").as_deref(),
            Some("https://x.test/?a=1&b=2")
        );
        assert_eq!(attribute(tag, "data-x").as_deref(), Some("plain"));
        assert_eq!(attribute(tag, "id"), None);
        // `data-href` is not `href`.
        assert_eq!(attribute("<a data-href=\"no\">", "href"), None);
    }

    #[test]
    fn elements_are_found_in_order_without_matching_longer_tag_names() {
        let html = "<abbr>x</abbr><a href=\"1\">one <b>bold</b></a> text <A href=\"2\">two</A>";
        let anchors = elements(html, "a");
        assert_eq!(anchors.len(), 2);
        assert_eq!(attribute(anchors[0].open_tag, "href").as_deref(), Some("1"));
        assert_eq!(inline_text(anchors[0].inner), "one bold");
        assert_eq!(inline_text(anchors[1].inner), "two");
    }
}
