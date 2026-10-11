# ADR-0060: A controlled browser for the agent

Status: **Accepted** (owner, 2026-10-11, Windows first).

## Context

`web_fetch` reads a page's HTML. It cannot use a site that needs a click, a form or a script to show its content, which is much of the modern web. ADR-0059 named a controlled browser, one that reads page structure instead of screen pixels, as the safer way to act on web pages and as the thing to build before any mouse-and-keyboard control (its step 3, still not decided).

## Decision

The desktop app drives a real Microsoft Edge or Google Chrome window through the Chrome DevTools Protocol. The code is in `crates/delta-computer/src/browser/`; `delta-core` and headless builds carry none of it. It adds no browser to the installer: Edge ships with Windows, and Chrome is searched for next. With neither, the tools say so.

| Tool | Asks the user | Does |
|---|---|---|
| `browser_navigate` | yes | Opens an `http(s)` address and returns the page |
| `browser_click`, `browser_type`, `browser_select`, `browser_press_key` | yes | Act on one element, or press one key |
| `browser_snapshot`, `browser_scroll`, `browser_back`, `browser_screenshot`, `browser_close` | no | Read, move around, close |

Acting tools are medium risk, category `computer`, so the policy never auto-allows them; the user can grant "always for this task" through the usual approval choices. The reading tools are low risk: they show the model only what a page already shows.

Rules:

1. **Delta's own profile.** The browser runs with its profile in `<state dir>/browser/profile`. The user's everyday cookies, history and saved passwords are not reachable. The window is visible so the user can watch, and logs in themselves; the login stays in Delta's profile.
2. **No secrets typed by the agent.** `browser_type` refuses password fields and fields whose `autocomplete` names a password or a card (`cc-*`), and snapshots do not show their values. The agent is told to ask the user to type those in the window.
3. **Elements are named and checked.** A snapshot numbers the visible buttons, links and fields (`e1`, `e2`, …) with their names. `click`, `type` and `select` must repeat the name; it is compared with the element actually under that number right before acting, so a page that changed cannot turn "Add to cart" into "Pay", and the approval card shows the name. A reference is validated as `e` plus digits before it can reach any script; typed text goes through the protocol, never into script source.
4. **The same network guard as `web_fetch`.** Every request the page makes (documents, scripts, images, fetches, frames and workers) is paused and vetted by `delta_core::web::guard`: loopback, private, link-local (the cloud metadata address included), carrier-grade NAT, multicast and reserved addresses are refused, also when carried inside an IPv6 address; `file:`, `chrome:`, `ftp:` and other schemes are refused. `data:`, `blob:` and `about:` pass. A name this machine cannot resolve is allowed, because the browser may reach it through the user's proxy. Verdicts are cached for a minute.
5. **Contained pages.** Windows the page opens by itself are closed (links that want a new window are opened in the same tab), downloads are denied, JavaScript dialogs are dismissed so they cannot freeze the page, and file upload is refused.
6. **Pages are external content.** The page's text is data to evaluate, never instructions; the tool descriptions and the system prompt say so, and the approval card says the page content goes to the model provider.
7. **Lifetime.** One browser for the app, one tab per task. It starts on first use, closes after ten idle minutes or when the user closes it, and ends with the app (on Windows through a job object, so it ends even if Delta crashes). A task's tab is closed with `browser_close`.

## Limits and what is not verified

- A page's own WebSocket connections are not seen by the request guard, and a name can change its answer between the guard's check and the browser's own lookup. Both are narrower than `web_fetch`'s pinned connection, which this design cannot do for a browser. The browser is not given anything on the user's network that a page in the user's everyday browser would not already reach, and the window shows what it does.
- Snapshots list at most 80 elements and 5,000 characters of text; content in cross-origin frames, shadow DOM and canvases is not listed. Use `browser_screenshot` for those.
- Tested against Chromium on Linux with a local server (form filling, label mismatches, password refusal, the guard, scrolling, back, screenshots, closing, idle close). Not run on Windows: Edge and the Windows job object were compiled for `x86_64-pc-windows-msvc` but never run there. Real sites with bot detection, login walls and pop-ups were not tried.
