//! The small scripts run inside the page. They are fixed text: the only things spliced in are a
//! validated element reference and JSON-encoded values, never page content or model text as code.

/// Shared by the snapshot and by every action, so the label the model saw and the label checked
/// before acting are computed the same way.
const HELPERS: &str = r#"
const clean = (t) => String(t || '').replace(/\s+/g, ' ').trim().slice(0, 80);
const labelOf = (el) => {
  let t = el.getAttribute('aria-label');
  if (!t && el.id) {
    try { const l = document.querySelector('label[for="' + CSS.escape(el.id) + '"]'); if (l) t = l.innerText; } catch (e) {}
  }
  if (!t && el.labels && el.labels.length) t = el.labels[0].innerText;
  if (!t && el.tagName === 'INPUT' && ['button', 'submit', 'reset'].includes(el.type)) t = el.value;
  if (!t) t = el.innerText;
  if (!t) { const img = el.querySelector && el.querySelector('img[alt]'); if (img) t = img.alt; }
  if (!t) t = el.getAttribute('title') || el.getAttribute('placeholder') || el.getAttribute('alt') || el.getAttribute('name') || '';
  return clean(t);
};
const isPassword = (el) => {
  const ac = (el.getAttribute('autocomplete') || '').toLowerCase();
  return (el.tagName === 'INPUT' && el.type === 'password') || ac.includes('password') || ac.startsWith('cc-');
};
"#;

/// Number the interactive elements (stamping `data-delta-ref`) and describe the page.
pub fn snapshot() -> String {
    format!(
        r#"(() => {{
{HELPERS}
const MAX_ELEMENTS = 80, MAX_TEXT = 5000;
for (const old of document.querySelectorAll('[data-delta-ref]')) old.removeAttribute('data-delta-ref');
const selector = 'a[href],button,input,select,textarea,summary,[role=button],[role=link],[role=checkbox],[role=radio],[role=tab],[role=menuitem],[role=option],[role=switch],[role=combobox],[contenteditable=""],[contenteditable=true],[onclick]';
const visible = (el) => {{
  const r = el.getBoundingClientRect();
  if (r.width < 2 || r.height < 2) return false;
  const s = getComputedStyle(el);
  return s.visibility !== 'hidden' && s.display !== 'none' && s.opacity !== '0';
}};
const elements = [];
let omitted = 0;
for (const el of document.querySelectorAll(selector)) {{
  if (el.tagName === 'INPUT' && el.type === 'hidden') continue;
  if (!visible(el)) continue;
  if (elements.length >= MAX_ELEMENTS) {{ omitted++; continue; }}
  const ref = 'e' + (elements.length + 1);
  el.setAttribute('data-delta-ref', ref);
  const item = {{ ref, tag: el.tagName.toLowerCase(), label: labelOf(el) }};
  if (el.tagName === 'INPUT') item.type = el.type;
  if (el.getAttribute('role')) item.role = el.getAttribute('role');
  if (el.tagName === 'A' && el.href) item.href = el.href.slice(0, 200);
  if (isPassword(el)) item.sensitive = true;
  else if (['INPUT', 'TEXTAREA', 'SELECT'].includes(el.tagName) && !['checkbox', 'radio', 'button', 'submit', 'reset', 'file'].includes(el.type)) {{
    const v = el.tagName === 'SELECT' ? (el.selectedOptions[0] ? el.selectedOptions[0].text : '') : el.value;
    if (v) item.value = String(v).slice(0, 80);
  }}
  if (el.tagName === 'SELECT') item.options = Array.from(el.options).slice(0, 30).map((o) => clean(o.text));
  if (el.type === 'checkbox' || el.type === 'radio') item.checked = !!el.checked;
  if (el.disabled) item.disabled = true;
  elements.push(item);
}}
const body = document.body ? document.body.innerText || '' : '';
const text = body.replace(/\n{{3,}}/g, '\n\n').trim();
return {{
  url: location.href,
  title: document.title,
  text: text.slice(0, MAX_TEXT),
  text_truncated: text.length > MAX_TEXT,
  elements,
  omitted_elements: omitted,
  scroll_y: Math.round(scrollY),
  page_height: Math.round(document.documentElement.scrollHeight),
  viewport_height: innerHeight,
}};
}})()"#
    )
}

/// Find an element by reference and describe it, so Rust can check it is the one meant.
pub fn locate(reference: &str) -> String {
    format!(
        r#"(() => {{
{HELPERS}
const el = document.querySelector('[data-delta-ref="{reference}"]');
if (!el || !el.isConnected) return {{ found: false }};
return {{
  found: true,
  tag: el.tagName.toLowerCase(),
  type: el.tagName === 'INPUT' ? el.type : '',
  label: labelOf(el),
  password: isPassword(el),
  editable: el.isContentEditable || el.tagName === 'TEXTAREA' || (el.tagName === 'INPUT' && !['checkbox', 'radio', 'button', 'submit', 'reset', 'file', 'image', 'range', 'color'].includes(el.type)),
  disabled: !!el.disabled,
}};
}})()"#
    )
}

/// Scroll the element into view and report where to click, and whether something covers it.
pub fn click_point(reference: &str) -> String {
    format!(
        r#"(() => {{
const el = document.querySelector('[data-delta-ref="{reference}"]');
if (!el) return {{ found: false }};
el.scrollIntoView({{ block: 'center', inline: 'center' }});
const a = el.closest('a');
if (a && a.target && a.target !== '_self') a.target = '_self';
const r = el.getBoundingClientRect();
const x = Math.round(r.left + r.width / 2), y = Math.round(r.top + r.height / 2);
const hit = document.elementFromPoint(x, y);
const covered = !hit || !(hit === el || el.contains(hit) || hit.contains(el));
return {{ found: true, x, y, covered }};
}})()"#
    )
}

/// Focus a text field and select what is in it, so inserted text replaces it. With `should_clear` the
/// field is emptied instead.
pub fn prepare_text(reference: &str, should_clear: bool) -> String {
    format!(
        r#"(() => {{
const el = document.querySelector('[data-delta-ref="{reference}"]');
if (!el) return {{ found: false }};
el.scrollIntoView({{ block: 'center', inline: 'center' }});
el.focus();
if (el.isContentEditable) {{
  const range = document.createRange();
  range.selectNodeContents(el);
  const sel = getSelection(); sel.removeAllRanges(); sel.addRange(range);
  if ({should_clear}) {{ document.execCommand('delete'); }}
}} else {{
  if ({should_clear}) {{
    el.value = '';
    el.dispatchEvent(new Event('input', {{ bubbles: true }}));
    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
  }} else {{
    el.select();
  }}
}}
return {{ found: true, focused: document.activeElement === el }};
}})()"#
    )
}

/// Choose an option of a drop-down by its visible text or its value. `wanted` is JSON text.
pub fn choose_option(reference: &str, wanted_json: &str) -> String {
    format!(
        r#"(() => {{
const el = document.querySelector('[data-delta-ref="{reference}"]');
if (!el || el.tagName !== 'SELECT') return {{ found: false }};
const wanted = {wanted_json};
const norm = (t) => String(t).replace(/\s+/g, ' ').trim().toLowerCase();
const options = Array.from(el.options);
const option = options.find((o) => norm(o.text) === norm(wanted)) || options.find((o) => norm(o.value) === norm(wanted));
if (!option) return {{ found: true, chosen: false, options: options.slice(0, 30).map((o) => o.text.trim().slice(0, 80)) }};
if (option.disabled) return {{ found: true, chosen: false, disabled: true }};
el.value = option.value;
el.dispatchEvent(new Event('input', {{ bubbles: true }}));
el.dispatchEvent(new Event('change', {{ bubbles: true }}));
return {{ found: true, chosen: true, text: option.text.trim().slice(0, 80) }};
}})()"#
    )
}

/// Scroll the page. `direction` is one of the fixed words the caller validated.
pub fn scroll(direction: &str) -> String {
    let call = match direction {
        "up" => "window.scrollBy(0, -Math.round(innerHeight * 0.8))",
        "top" => "window.scrollTo(0, 0)",
        "bottom" => "window.scrollTo(0, document.documentElement.scrollHeight)",
        _ => "window.scrollBy(0, Math.round(innerHeight * 0.8))",
    };
    format!("(() => {{ {call}; return true; }})()")
}

pub fn ready_state() -> &'static str {
    "document.readyState"
}
