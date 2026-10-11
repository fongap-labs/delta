# ADR-0059: Computer capabilities, in steps

Status: **Accepted for steps 1 and 2** (owner, 2026-10-11, Windows first). Step 3 is **not accepted**: it needs its own decision.

## Context

Some office work happens in programs that offer no file or API to read: an error dialog, a legacy desktop app, a window the user is looking at. Other agents solve this with "computer use": the model sees the screen and then moves the mouse and types. That is the most powerful and the most dangerous thing an agent can do: it acts as the user, in any program, including banking, mail and chat, and what is on the screen goes to the model provider.

The blueprint puts human control of consequences first (4.3) and requires every capability to run under Trust, Policy and Approval (section 10). The runtime today gives the model five tools plus the document and web tools of ADR-0058; it cannot look at the screen, and a tool result cannot carry a picture.

## Decision

Computer capabilities arrive in three steps. Only the first two are decided here.

**Step 1 and 2 (built):** read-mostly capabilities, each asking the user every time (medium risk, category `computer`, so the policy never auto-allows them):

| Tool | Does |
|---|---|
| `screenshot` | One picture of the main display or all displays, shrunk to at most 1568 px on the long side, kept in `.delta/screens/<run>/` as a record of what the agent saw (deleted after 7 days), and attached to the model's next message |
| `clipboard_read` / `clipboard_write` | Text only, up to 100,000 characters |
| `open_file` | Opens a workspace file in the user's own program, only documents, text and common images (the list of the artifact "Open" button); anything else can only be revealed in the file manager |
| `open_url` | Opens an `http(s)` address in the user's browser; no credentials, no other schemes |

Rules:

1. **Where the code lives.** Platform code is in its own crate, `delta-computer`, registered by the desktop app; `delta-core` and headless builds carry none of it and need no display library. Windows is implemented (GDI capture). Other systems answer "not supported yet" for screenshots; clipboard and open use cross-platform libraries.
2. **Pictures reach the model through the transcript, not through the tool result.** A tool lists its pictures as `images: [{path}]` under `.delta/screens/`; the runtime reads them back at request time, sends only the newest two, and puts them in one user message after the last tool result (OpenAI-style APIs require tool results to stay together; Anthropic and Responses message shapes are converted the same way). A path outside `.delta/screens/`, or one that does not exist, is ignored.
3. **Disclosure.** The approval card says what the call does and that what it sees goes to the model provider.
4. **Vision.** The model must read images. The runtime does not know which models do; a model that cannot will make the provider refuse the request, and the error is shown.

## Step 3 (not decided): acting on the screen

Mouse and keyboard control stays out until the owner accepts a consent design. The minimum that design must contain: a master switch in Settings that is off by default; a persistent on-screen "Delta is controlling this computer" indicator and an Esc kill switch; an allow-list of programs, with a first-use approval per program and banking, password managers and system settings refused; every step's screenshot kept; per-action approval at first and relaxation only by explicit, revocable grants; Windows UI Automation (named controls) preferred over pixel coordinates; and a verification that works on Windows, which this change could only compile for.

For web pages, a controlled browser that reads the page structure (the Python `integrations/connectors/browser_automation.py` is the reference) is safer and more reliable than clicking pixels, and should come before step 3.

## Not verified

The Windows screen capture was compiled for `x86_64-pc-windows-msvc` and its pure parts (shrinking, PNG encoding, path and address checks, the tool plumbing through a fake screen) are tested; it was never run on Windows. Capture of protected content (the lock screen, a UAC prompt, some full-screen games and DRM video) returns an error or a black picture.
