//! Turn an HTML page into Markdown for the model.
//!
//! `html::html_to_text` only strips tags, so a page's menus, footers and cookie banners fill the
//! reader's budget, its links are lost (the model cannot follow them) and headings, lists, tables
//! and code are flattened. This module keeps the structure that carries meaning (headings, lists,
//! links, tables, code, quotes) and leaves out the page furniture.
//!
//! Like `html`, it is a forgiving scanner, not a parser: it never fails, and a page it cannot make
//! sense of yields little text rather than an error. The page is converted up to three ways and
//! the narrowest result that still holds a fair share of the page's text wins, so a wrongly
//! recognised "menu" can never empty a page:
//!
//! 1. only inside `<main>` (or, without one, `<article>`), without page furniture;
//! 2. the whole page, without page furniture;
//! 3. the whole page.

use url::Url;

use super::html::{
    attribute, collapse_whitespace, decode_entities, hides_element, tag_name, SKIPPED, VOID,
};

/// Most links returned for one page.
pub const MAX_LINKS: usize = 100;
/// Most table rows written out; the rest are counted.
const MAX_TABLE_ROWS: usize = 200;
/// A table cell longer than this makes the table a page layout, not data.
const MAX_DATA_CELL_CHARS: usize = 240;

/// A link in the readable part of a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub text: String,
    /// Absolute `http` or `https` address.
    pub url: String,
}

#[derive(Debug)]
pub struct Extracted {
    pub markdown: String,
    pub links: Vec<Link>,
    /// The page has almost no text of its own and looks like a script-built shell.
    pub needs_javascript: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Everything,
    /// Only inside `<main>` / `role="main"`.
    Main,
    /// Only inside `<article>`.
    Article,
}

#[derive(Debug, Clone, Copy)]
struct Mode {
    focus: Focus,
    /// Drop navigation, footers, sidebars, cookie banners and other page furniture.
    filter: bool,
}

/// Convert `html` (a page at `base`) to Markdown, choosing how much of the page to keep.
pub fn html_to_markdown(html: &str, base: &Url) -> Extracted {
    let lower = html.to_ascii_lowercase();
    let full = convert(
        html,
        base,
        Mode {
            focus: Focus::Everything,
            filter: false,
        },
    );
    let full_len = full.markdown.chars().count();
    let focus = if lower.contains("<main")
        || lower.contains("role=\"main\"")
        || lower.contains("role='main'")
    {
        Focus::Main
    } else if lower.contains("<article") {
        Focus::Article
    } else {
        Focus::Everything
    };
    let mut chosen = None;
    if focus != Focus::Everything {
        let focused = convert(
            html,
            base,
            Mode {
                focus,
                filter: true,
            },
        );
        if focused.markdown.chars().count() >= 300.max(full_len / 5) {
            chosen = Some(focused);
        }
    }
    if chosen.is_none() {
        let filtered = convert(
            html,
            base,
            Mode {
                focus: Focus::Everything,
                filter: true,
            },
        );
        if filtered.markdown.chars().count() >= 150.max(full_len / 4) {
            chosen = Some(filtered);
        }
    }
    let mut result = chosen.unwrap_or(full);
    result.needs_javascript = result.markdown.chars().count() < 200 && looks_script_built(&lower);
    result
}

/// Whether a page with little text is a shell that scripts fill in.
fn looks_script_built(lower: &str) -> bool {
    const MARKERS: [&str; 9] = [
        "id=\"root\"",
        "id=\"app\"",
        "id=\"__next\"",
        "__next_data__",
        "data-reactroot",
        "ng-version",
        "enable javascript",
        "javascript is required",
        "requires javascript",
    ];
    MARKERS.iter().any(|marker| lower.contains(marker)) || lower.matches("<script").count() >= 3
}

fn convert(html: &str, base: &Url, mode: Mode) -> Extracted {
    let mut converter = Converter {
        base,
        mode,
        stack: Vec::new(),
        skipping: false,
        focus_depth: 0,
        content_depth: 0,
        sinks: vec![Sink::default()],
        lists: Vec::new(),
        quote_depth: 0,
        pre_depth: 0,
        tables: Vec::new(),
        link_starts: Vec::new(),
        links: Vec::new(),
    };
    converter.run(html);
    let markdown = tidy(&converter.sinks.swap_remove(0).buf);
    Extracted {
        markdown,
        links: converter.links,
        needs_javascript: false,
    }
}

/// Trailing spaces off every line, at most one blank line in a row, no blank lines at the ends.
fn tidy(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank_run = 0;
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            blank_run += 1;
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
            if blank_run > 0 {
                out.push('\n');
            }
        }
        blank_run = 0;
        out.push_str(line);
    }
    out
}

#[derive(Default)]
struct Sink {
    buf: String,
    line_start: bool,
}

struct List {
    ordered: bool,
    count: usize,
}

#[derive(Default)]
struct Cell {
    text: String,
}

#[derive(Default)]
struct Table {
    rows: Vec<Vec<Cell>>,
    row: Vec<Cell>,
    contains_table: bool,
}

struct LinkStart {
    sink: usize,
    at: usize,
    url: String,
}

/// An open element and what has to be undone when it closes.
#[derive(Default)]
struct Open {
    name: String,
    /// This element started a skipped subtree.
    skip_root: bool,
    focus: bool,
    content: bool,
    list: bool,
    quote: bool,
    pre: bool,
    link: bool,
    table: bool,
    row: bool,
    cell: bool,
    code: bool,
    heading: bool,
    block: bool,
    paragraph: bool,
}

struct Converter<'a> {
    base: &'a Url,
    mode: Mode,
    stack: Vec<Open>,
    skipping: bool,
    focus_depth: usize,
    content_depth: usize,
    sinks: Vec<Sink>,
    lists: Vec<List>,
    quote_depth: usize,
    pre_depth: usize,
    tables: Vec<Table>,
    link_starts: Vec<LinkStart>,
    links: Vec<Link>,
}

/// The end of the tag that starts `at`: the first `>` outside quotes, or the first `>` at all when
/// a quote never closes.
fn tag_end(at: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (index, c) in at.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') if index > 0 => quote = Some(c),
            (Some(open), _) if c == open => quote = None,
            (None, '>') => return Some(index),
            _ => {}
        }
    }
    at.find('>')
}

/// Where the raw-text element `name` (script, style) ends in `rest`: the offset after its closing
/// tag, or the end of `rest`.
fn after_raw_text(rest: &str, name: &str) -> usize {
    let lower = rest.to_ascii_lowercase();
    let close = format!("</{name}");
    match lower.find(&close) {
        Some(at) => lower[at..].find('>').map_or(rest.len(), |gt| at + gt + 1),
        None => rest.len(),
    }
}

impl Converter<'_> {
    fn run(&mut self, html: &str) {
        let mut rest = html;
        while let Some(lt) = rest.find('<') {
            self.text(&rest[..lt]);
            let at = &rest[lt..];
            if let Some(after) = at.strip_prefix("<!--") {
                rest = after.find("-->").map_or("", |end| &after[end + 3..]);
                continue;
            }
            if at.starts_with("<!") || at.starts_with("<?") {
                rest = at.find('>').map_or("", |gt| &at[gt + 1..]);
                continue;
            }
            let Some(gt) = tag_end(at) else {
                rest = "";
                break;
            };
            let tag = &at[..=gt];
            let (name, is_closing) = tag_name(tag);
            if name.is_empty() {
                self.text("<");
                rest = &at[1..];
                continue;
            }
            rest = &at[gt + 1..];
            if is_closing {
                self.close(&name);
            } else if matches!(name.as_str(), "script" | "style") && !tag.ends_with("/>") {
                // The text of a script is code, not markup: ignore everything up to its end.
                rest = &rest[after_raw_text(rest, &name)..];
            } else {
                self.open(&name, tag);
            }
        }
        self.text(rest);
        while let Some(element) = self.stack.pop() {
            self.finish(element);
        }
    }

    // --- state ---------------------------------------------------------------------------

    fn emitting(&self) -> bool {
        !self.skipping && (self.mode.focus == Focus::Everything || self.focus_depth > 0)
    }

    fn sink(&mut self) -> &mut Sink {
        self.sinks.last_mut().expect("there is always one sink")
    }

    fn in_cell(&self) -> bool {
        self.sinks.len() > 1
    }

    /// What starts every line of text written now: quote marks and list indentation.
    fn prefix(&self) -> String {
        if self.in_cell() {
            return String::new();
        }
        let mut prefix = "> ".repeat(self.quote_depth);
        prefix.push_str(&"  ".repeat(self.lists.len()));
        prefix
    }

    fn raw(&mut self, text: &str) {
        let sink = self.sink();
        sink.buf.push_str(text);
        sink.line_start = text.ends_with('\n');
    }

    fn write_text(&mut self, text: &str) {
        if self.sink().line_start || self.sink().buf.is_empty() {
            let prefix = self.prefix();
            self.raw(&prefix);
        }
        self.raw(text);
    }

    fn newline(&mut self) {
        let sink = self.sink();
        let kept = sink.buf.trim_end_matches([' ', '\t']).len();
        sink.buf.truncate(kept);
        if !sink.buf.is_empty() && !sink.buf.ends_with('\n') {
            sink.buf.push('\n');
        }
        sink.line_start = true;
    }

    fn blank(&mut self) {
        self.newline();
        let sink = self.sink();
        if !sink.buf.is_empty() && !sink.buf.ends_with("\n\n") {
            sink.buf.push('\n');
        }
        sink.line_start = true;
    }

    fn space(&mut self) {
        let sink = self.sink();
        if !sink.line_start && !sink.buf.is_empty() && !sink.buf.ends_with([' ', '\n']) {
            sink.buf.push(' ');
        }
    }

    /// Several lines of already-converted text, as a block.
    fn block_text(&mut self, text: &str) {
        self.blank();
        for line in text.lines() {
            if line.trim().is_empty() {
                self.blank();
            } else {
                self.write_text(line.trim_end());
                self.newline();
            }
        }
    }

    // --- text ----------------------------------------------------------------------------

    fn text(&mut self, raw: &str) {
        if raw.is_empty() || !self.emitting() {
            return;
        }
        let decoded = decode_entities(raw);
        if self.pre_depth > 0 {
            self.raw(&decoded);
            return;
        }
        let core = collapse_whitespace(&decoded);
        if core.is_empty() {
            self.space();
            return;
        }
        if decoded.starts_with(char::is_whitespace) {
            self.space();
        }
        self.write_text(&core);
        if decoded.ends_with(char::is_whitespace) {
            self.space();
        }
    }

    // --- elements ------------------------------------------------------------------------

    /// Close the outermost open element named in `names` that is not behind an element named in
    /// `stop` (and everything inside it). Used for end tags HTML lets a page leave out: a new row
    /// ends the open cell and the open row.
    fn close_implied(&mut self, names: &[&str], stop: &[&str]) {
        let mut target = None;
        for index in (0..self.stack.len()).rev() {
            let name = self.stack[index].name.as_str();
            if names.contains(&name) {
                target = Some(index);
            } else if stop.contains(&name) {
                break;
            }
        }
        if let Some(index) = target {
            self.close_to(index);
        }
    }

    fn close_to(&mut self, index: usize) {
        while self.stack.len() > index {
            let element = self.stack.pop().expect("checked length");
            self.finish(element);
        }
    }

    fn close(&mut self, name: &str) {
        if let Some(index) = self.stack.iter().rposition(|open| open.name == name) {
            self.close_to(index);
        }
    }

    fn should_skip(&self, name: &str, tag: &str) -> bool {
        if hides_element(tag) || SKIPPED.contains(&name) {
            return true;
        }
        if !self.mode.filter {
            return false;
        }
        if attribute(tag, "aria-hidden").is_some_and(|v| v.eq_ignore_ascii_case("true")) {
            return true;
        }
        let is_in_content = self.content_depth > 0;
        let by_tag = match name {
            "nav" | "dialog" | "button" | "select" | "option" | "iframe" | "canvas" | "audio"
            | "video" | "object" | "embed" | "menu" => true,
            "footer" | "aside" | "header" => !is_in_content,
            _ => false,
        };
        if by_tag {
            return true;
        }
        let role = attribute(tag, "role")
            .unwrap_or_default()
            .to_ascii_lowercase();
        match role.as_str() {
            "navigation" | "dialog" | "alertdialog" | "search" => return true,
            "contentinfo" | "complementary" | "banner" if !is_in_content => return true,
            _ => {}
        }
        if matches!(
            name,
            "html"
                | "body"
                | "main"
                | "article"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "p"
                | "table"
                | "pre"
                | "code"
                | "a"
        ) {
            return false;
        }
        furniture_class(tag, is_in_content)
    }

    fn open(&mut self, name: &str, tag: &str) {
        match name {
            "li" => self.close_implied(&["li"], &["ul", "ol"]),
            "p" => self.close_implied(
                &["p"],
                &[
                    "div",
                    "section",
                    "article",
                    "td",
                    "th",
                    "li",
                    "blockquote",
                    "body",
                ],
            ),
            "td" | "th" => self.close_implied(&["td", "th"], &["tr", "table"]),
            "tr" => self.close_implied(&["td", "th", "tr"], &["table", "tbody", "thead", "tfoot"]),
            "tbody" | "thead" | "tfoot" => {
                self.close_implied(&["td", "th", "tr", "tbody", "thead", "tfoot"], &["table"]);
            }
            "dt" | "dd" => self.close_implied(&["dt", "dd"], &["dl"]),
            "body" => {
                if let Some(index) = self.stack.iter().rposition(|open| open.name == "head") {
                    self.close_to(index);
                }
            }
            _ => {}
        }
        let is_void = VOID.contains(&name) || tag.ends_with("/>");
        let mut element = Open {
            name: name.to_owned(),
            ..Open::default()
        };
        if !self.skipping && self.should_skip(name, tag) {
            self.skipping = true;
            element.skip_root = true;
        }
        let role_main = attribute(tag, "role").is_some_and(|v| v.eq_ignore_ascii_case("main"));
        if !self.skipping {
            let focus_here = match self.mode.focus {
                Focus::Main => name == "main" || role_main,
                Focus::Article => name == "article",
                Focus::Everything => false,
            };
            if focus_here {
                element.focus = true;
                self.focus_depth += 1;
            }
            if name == "main" || name == "article" || role_main {
                element.content = true;
                self.content_depth += 1;
            }
        }
        if self.emitting() {
            self.start(name, tag, &mut element);
        }
        if is_void {
            self.finish(element);
        } else {
            self.stack.push(element);
        }
    }

    /// What an opening tag writes. Only called while output is wanted.
    fn start(&mut self, name: &str, tag: &str, element: &mut Open) {
        match name {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = usize::from(name.as_bytes()[1] - b'0');
                self.blank();
                self.write_text(&format!("{} ", "#".repeat(level)));
                element.heading = true;
            }
            "p" => {
                self.blank();
                element.paragraph = true;
            }
            "div" | "section" | "article" | "main" | "header" | "footer" | "aside" | "form"
            | "figure" | "figcaption" | "address" | "details" | "summary" | "fieldset" | "dl"
            | "dt" | "dd" | "center" => {
                self.newline();
                element.block = true;
            }
            "ul" | "ol" => {
                if self.lists.is_empty() {
                    self.blank();
                } else {
                    self.newline();
                }
                let start = attribute(tag, "start")
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .map_or(0, |n| n.saturating_sub(1));
                self.lists.push(List {
                    ordered: name == "ol",
                    count: start,
                });
                element.list = true;
            }
            "li" => {
                self.newline();
                let depth = self.lists.len().saturating_sub(1);
                let marker = match self.lists.last_mut() {
                    Some(list) if list.ordered => {
                        list.count += 1;
                        format!("{}. ", list.count)
                    }
                    _ => "- ".to_owned(),
                };
                let line = if self.in_cell() {
                    marker
                } else {
                    format!(
                        "{}{}{marker}",
                        "> ".repeat(self.quote_depth),
                        "  ".repeat(depth)
                    )
                };
                self.raw(&line);
                element.block = true;
            }
            "blockquote" => {
                self.blank();
                self.quote_depth += 1;
                element.quote = true;
            }
            "pre" => {
                self.blank();
                let language = code_language(tag).unwrap_or_default();
                self.raw(&format!("```{language}\n"));
                self.pre_depth += 1;
                element.pre = true;
            }
            "code" if self.pre_depth == 0 => {
                self.raw("`");
                element.code = true;
            }
            "code" => {
                // Inside <pre>, the language is often on the <code> instead of the <pre>.
                if let Some(language) = code_language(tag) {
                    let sink = self.sink();
                    if sink.buf.ends_with("```\n") {
                        let len = sink.buf.len();
                        sink.buf.truncate(len - 1);
                        sink.buf.push_str(&format!("{language}\n"));
                    }
                }
            }
            "br" => {
                if self.pre_depth > 0 {
                    self.raw("\n");
                } else if self.in_cell() {
                    self.space();
                } else {
                    self.newline();
                }
            }
            "hr" => {
                self.blank();
                self.write_text("---");
                self.blank();
            }
            "img" => {
                let alt = attribute(tag, "alt").unwrap_or_default();
                let alt = collapse_whitespace(&alt);
                if alt.chars().count() >= 4 && self.pre_depth == 0 {
                    self.space();
                    self.write_text(&format!("[Image: {alt}]"));
                    self.space();
                }
            }
            "a" if self.pre_depth == 0 => {
                if let Some(url) = attribute(tag, "href").and_then(|href| self.link_target(&href)) {
                    let at = self.sink().buf.len();
                    self.link_starts.push(LinkStart {
                        sink: self.sinks.len() - 1,
                        at,
                        url,
                    });
                    element.link = true;
                }
            }
            "table" => {
                if let Some(outer) = self.tables.last_mut() {
                    outer.contains_table = true;
                }
                self.blank();
                self.tables.push(Table::default());
                element.table = true;
            }
            "tr" if !self.tables.is_empty() => {
                element.row = true;
            }
            "td" | "th" if !self.tables.is_empty() => {
                self.sinks.push(Sink {
                    buf: String::new(),
                    line_start: true,
                });
                element.cell = true;
            }
            "td" | "th" | "tr" => {
                self.newline();
                element.block = true;
            }
            _ => {}
        }
    }

    fn link_target(&self, href: &str) -> Option<String> {
        let href = href.trim();
        if href.is_empty() || href.starts_with('#') {
            return None;
        }
        let url = self.base.join(href).ok()?;
        matches!(url.scheme(), "http" | "https").then(|| url.to_string())
    }

    /// Undo what an element's opening tag set up, and write what its end writes.
    fn finish(&mut self, element: Open) {
        if element.skip_root {
            self.skipping = false;
        }
        if element.focus {
            self.focus_depth -= 1;
        }
        if element.content {
            self.content_depth -= 1;
        }
        if element.heading || element.paragraph {
            self.blank();
        }
        if element.block {
            self.newline();
        }
        if element.code {
            self.raw("`");
        }
        if element.link {
            self.finish_link();
        }
        if element.list {
            self.lists.pop();
            if self.lists.is_empty() {
                self.blank();
            } else {
                self.newline();
            }
        }
        if element.quote {
            self.quote_depth -= 1;
            self.blank();
        }
        if element.pre {
            self.pre_depth -= 1;
            self.newline();
            self.raw("```\n");
            self.blank();
        }
        if element.cell {
            let cell = self.sinks.pop().expect("a cell has its own sink");
            if let Some(table) = self.tables.last_mut() {
                table.row.push(Cell {
                    text: cell.buf.trim().to_owned(),
                });
            }
        }
        if element.row {
            if let Some(table) = self.tables.last_mut() {
                if !table.row.is_empty() {
                    let row = std::mem::take(&mut table.row);
                    table.rows.push(row);
                }
            }
        }
        if element.table {
            if let Some(table) = self.tables.pop() {
                self.write_table(table);
            }
        }
    }

    fn finish_link(&mut self) {
        let Some(start) = self.link_starts.pop() else {
            return;
        };
        if start.sink != self.sinks.len() - 1 || start.at > self.sink().buf.len() {
            return;
        }
        let written = self.sink().buf.split_off(start.at);
        let flat = collapse_whitespace(&written);
        // A heading or list item opened inside the link keeps its mark outside the brackets.
        let (mark, label) = split_leading_mark(&flat);
        let label = label.trim();
        let mut out = mark.to_owned();
        if label.is_empty() {
            self.raw(&out);
            return;
        }
        if label == start.url {
            out.push_str(label);
        } else {
            out.push_str(&format!("[{label}]({})", start.url));
        }
        let sink = self.sink();
        if sink.line_start && !out.is_empty() {
            sink.line_start = false;
        }
        sink.buf.push_str(&out);
        if self.pre_depth == 0 && self.emitting() && self.links.len() < MAX_LINKS {
            let url = start.url;
            if !self.links.iter().any(|link| link.url == url) {
                self.links.push(Link {
                    text: label.chars().take(120).collect(),
                    url,
                });
            }
        }
    }

    fn write_table(&mut self, table: Table) {
        if !self.emitting() {
            return;
        }
        let columns = table.rows.iter().map(Vec::len).max().unwrap_or(0);
        let is_data = !table.contains_table
            && table.rows.len() >= 2
            && columns >= 2
            && table
                .rows
                .iter()
                .flatten()
                .all(|cell| cell.text.chars().count() <= MAX_DATA_CELL_CHARS);
        if is_data {
            self.blank();
            for (index, row) in table.rows.iter().take(MAX_TABLE_ROWS).enumerate() {
                let cells: Vec<String> = (0..columns)
                    .map(|column| {
                        row.get(column)
                            .map(|cell| collapse_whitespace(&cell.text.replace('|', "\\|")))
                            .unwrap_or_default()
                    })
                    .collect();
                self.write_text(&format!("| {} |", cells.join(" | ")));
                self.newline();
                if index == 0 {
                    self.write_text(&format!("|{}", " --- |".repeat(columns)));
                    self.newline();
                }
            }
            if table.rows.len() > MAX_TABLE_ROWS {
                self.write_text(&format!(
                    "({} more rows not shown)",
                    table.rows.len() - MAX_TABLE_ROWS
                ));
                self.newline();
            }
            self.blank();
        } else {
            for cell in table.rows.iter().flatten() {
                if !cell.text.is_empty() {
                    self.block_text(&cell.text);
                }
            }
            self.blank();
        }
    }
}

/// `## ` or `- ` at the start of a link's text, and the rest.
fn split_leading_mark(text: &str) -> (&str, &str) {
    let hashes = text.chars().take_while(|c| *c == '#').count();
    if hashes > 0 && text[hashes..].starts_with(' ') {
        return text.split_at(hashes + 1);
    }
    if let Some(rest) = text.strip_prefix("- ") {
        return (&text[..2], rest);
    }
    ("", text)
}

/// The language named by a `language-x` or `lang-x` class.
fn code_language(tag: &str) -> Option<String> {
    let class = attribute(tag, "class")?;
    class.split_whitespace().find_map(|token| {
        let token = token.to_ascii_lowercase();
        let language = token
            .strip_prefix("language-")
            .or_else(|| token.strip_prefix("lang-"))?;
        let ok = !language.is_empty()
            && language
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '#' | '-' | '_'));
        ok.then(|| language.to_owned())
    })
}

/// Words in a class or id that mark page furniture wherever it is.
const FURNITURE_ANYWHERE: [&str; 9] = [
    "cookie",
    "consent",
    "gdpr",
    "popup",
    "newsletter",
    "adsbygoogle",
    "skiplink",
    "skip-link",
    "paywall-overlay",
];
/// Whole class words that mark furniture wherever it is.
const FURNITURE_WORDS: [&str; 4] = ["modal", "advert", "advertisement", "ad"];
/// Whole class words that mark furniture outside the article.
const FURNITURE_OUTSIDE: [&str; 13] = [
    "sidebar",
    "navbar",
    "navigation",
    "menu",
    "footer",
    "breadcrumb",
    "breadcrumbs",
    "share",
    "sharing",
    "social",
    "related",
    "toolbar",
    "topbar",
];

fn furniture_class(tag: &str, is_in_content: bool) -> bool {
    let mut values = attribute(tag, "class").unwrap_or_default();
    values.push(' ');
    values.push_str(&attribute(tag, "id").unwrap_or_default());
    let values = values.to_ascii_lowercase();
    if FURNITURE_ANYWHERE
        .iter()
        .any(|marker| values.contains(marker))
    {
        return true;
    }
    let words: Vec<&str> = values
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    if words.iter().any(|word| FURNITURE_WORDS.contains(word)) {
        return true;
    }
    !is_in_content && words.iter().any(|word| FURNITURE_OUTSIDE.contains(word))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn md(html: &str) -> Extracted {
        html_to_markdown(
            html,
            &Url::parse("https://example.org/docs/page.html").unwrap(),
        )
    }

    const ARTICLE: &str = r##"<!doctype html><html><head><title>T</title>
<style>.x{color:red}</style><script>var s = "</div></main>"; if (a<b) {}</script></head>
<body>
<div class="cookie-banner">We use cookies. <button>Accept</button></div>
<header><nav><a href="/">Home</a> <a href="/about">About</a></nav></header>
<aside class="sidebar"><a href="/ads">Buy things</a></aside>
<main><article>
<h1>Guide to Widgets</h1>
<p>Widgets are <b>small</b> parts. See the <a href="../ref/widget.html">reference</a>,
the <a href="#top">top</a>, <a href="javascript:void(0)">script</a> and <a href="mailto:a@b.c">mail</a>.</p>
<h2>Steps</h2>
<ol><li>First<ul><li>nested one</li><li>nested two</li></ul></li><li>Second</li></ol>
<pre><code class="language-rust">fn main() {
    println!("hi");
}</code></pre>
<table><tr><th>Name</th><th>Size</th></tr><tr><td>a|b</td><td>10</td></tr></table>
<blockquote><p>Quoted words.</p></blockquote>
<img src="x.png" alt="A diagram of a widget"><img src="y.png" alt="">
</article></main>
<footer><a href="/privacy">Privacy</a> Copyright 2026</footer>
</body></html>"##;

    #[test]
    fn an_article_keeps_its_structure_and_loses_the_page_furniture() {
        let out = md(ARTICLE);
        let text = out.markdown;
        assert!(
            text.starts_with("# Guide to Widgets\n\nWidgets are small parts."),
            "{text}"
        );
        assert!(text.contains("## Steps"), "{text}");
        assert!(
            text.contains("1. First\n  - nested one\n  - nested two\n2. Second"),
            "{text}"
        );
        assert!(
            text.contains("```rust\nfn main() {\n    println!(\"hi\");\n}\n```"),
            "{text}"
        );
        assert!(
            text.contains("| Name | Size |\n| --- | --- |\n| a\\|b | 10 |"),
            "{text}"
        );
        assert!(text.contains("> Quoted words."), "{text}");
        assert!(text.contains("[Image: A diagram of a widget]"), "{text}");
        for furniture in [
            "cookies",
            "Accept",
            "Home",
            "About",
            "Buy things",
            "Privacy",
            "Copyright",
            "color:red",
        ] {
            assert!(
                !text.contains(furniture),
                "{furniture} is page furniture:\n{text}"
            );
        }
    }

    #[test]
    fn links_are_made_absolute_and_only_real_web_links_are_kept() {
        let out = md(ARTICLE);
        assert!(
            out.markdown
                .contains("[reference](https://example.org/ref/widget.html)"),
            "{}",
            out.markdown
        );
        assert!(
            out.markdown.contains("the top, script and mail."),
            "{}",
            out.markdown
        );
        assert_eq!(
            out.links,
            vec![Link {
                text: "reference".to_owned(),
                url: "https://example.org/ref/widget.html".to_owned()
            }]
        );
    }

    #[test]
    fn furniture_is_dropped_on_a_page_with_no_main_or_article() {
        let page = r#"<body><div class="top-nav"><nav><a href="/x">Menu item</a></nav></div>
<div id="cookie-consent">Accept all</div>
<div class="post"><h2>Release notes</h2><p>The release fixes forty bugs and adds a dark mode to the
settings page, which many readers asked for over the last year of discussion on the forum.</p>
<p>Upgrade notes follow below with the details of every change that was made to the file format.</p></div>
<div class="footer-links"><a href="/t">Terms of service</a></div></body>"#;
        let out = md(page).markdown;
        assert!(out.contains("## Release notes"), "{out}");
        assert!(out.contains("adds a dark mode"), "{out}");
        for gone in ["Menu item", "Accept all", "Terms of service"] {
            assert!(!out.contains(gone), "{gone}:\n{out}");
        }
    }

    #[test]
    fn a_page_is_never_emptied_by_a_wrongly_recognised_menu() {
        // A restaurant's whole page is "the menu": filtering it would leave nothing.
        let page = r#"<body><div class="menu"><h1>Dinner menu</h1><p>Grilled sea bass with lemon and
capers, served with roasted potatoes and seasonal greens, forty dollars per person.</p>
<p>Wild mushroom risotto with parmesan and truffle oil, thirty dollars per person.</p></div></body>"#;
        let out = md(page).markdown;
        assert!(out.contains("Dinner menu"), "{out}");
        assert!(out.contains("Wild mushroom risotto"), "{out}");
    }

    #[test]
    fn a_layout_table_is_read_as_paragraphs_and_a_data_table_as_a_table() {
        let layout = r#"<body><table><tr><td><table><tr><td>Left column text</td></tr></table></td>
<td>Right column text with a <a href="/z">link</a></td></tr></table></body>"#;
        let out = md(layout).markdown;
        assert!(
            out.contains(
                "Left column text\n\nRight column text with a [link](https://example.org/z)"
            ),
            "{out}"
        );
        assert!(!out.contains('|'), "{out}");
    }

    #[test]
    fn unclosed_list_items_paragraphs_and_cells_are_closed_for_the_page() {
        let page = "<body><ul><li>one<li>two<li>three</ul><p>first<p>second</body>";
        assert_eq!(
            md(page).markdown,
            "- one\n- two\n- three\n\nfirst\n\nsecond"
        );
        let table = "<table><tr><th>A<th>B<tr><td>1<td>2</table>";
        assert_eq!(md(table).markdown, "| A | B |\n| --- | --- |\n| 1 | 2 |");
    }

    #[test]
    fn hidden_text_and_entities_are_handled_like_the_plain_text_reader() {
        let page = r#"<p>Fish &amp; chips &#x4e2d;</p><div style="display:none">secret</div><p hidden>no</p>"#;
        assert_eq!(md(page).markdown, "Fish & chips 中");
    }

    #[test]
    fn a_heading_link_keeps_the_heading_mark_outside_the_brackets() {
        let page =
            r#"<body><a href="/post"><h2>Post title</h2></a><p>Body text of the post.</p></body>"#;
        let out = md(page).markdown;
        assert!(
            out.starts_with("## [Post title](https://example.org/post)"),
            "{out}"
        );
    }

    #[test]
    fn a_script_built_shell_is_recognised() {
        let shell =
            r#"<html><body><div id="root"></div><script src="/a.js"></script></body></html>"#;
        let out = md(shell);
        assert!(out.needs_javascript);
        let real = format!(
            "<body><p>{}</p></body>",
            "A sentence of real text. ".repeat(20)
        );
        assert!(!md(&real).needs_javascript);
    }

    #[test]
    fn several_articles_on_an_index_page_are_all_kept() {
        let one = |n: u32| {
            format!(
                "<article><h2>Post {n}</h2><p>{}</p></article>",
                "Some words about the post. ".repeat(6)
            )
        };
        let page = format!(
            "<body><nav>Site menu</nav>{}{}{}</body>",
            one(1),
            one(2),
            one(3)
        );
        let out = md(&page).markdown;
        for n in 1..=3 {
            assert!(out.contains(&format!("## Post {n}")), "{out}");
        }
        assert!(!out.contains("Site menu"), "{out}");
    }
}
