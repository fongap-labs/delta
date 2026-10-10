# Changelog

## [Unreleased]

- fix: the Activity page, the Inbox and the Memory page no longer present a failed read as an empty list ("No audit events yet", an empty queue, "Nothing yet"). They now say the data could not be loaded and offer a retry, and show a skeleton while a first read is slow. The read functions throw when the runtime reports an error instead of returning an empty result.

- fix: while a run is waiting for an approval, a folder request, a plan decision or a question, the run status bar now says "Awaiting approval" with a hand icon instead of "Running", so a blocked run is no longer shown as working.
- feat: the artifact viewer shows a trust strip when the runtime recorded how the file was made: the validation verdict (checks passed or failed, expandable to each check and its reason), the tool that produced it, and the SHA-256 registered with it. It is read from the producing tool result already stored in the conversation, so nothing is inferred and a file without such a record shows no strip. Sources and versions are not shown yet.

- feat: the macOS menu-bar icon is now a single-colour template image, so it follows the light and dark menu bar and the highlight state; Windows and Linux keep the full-colour brand icon. The template is derived from the brand source by `scripts/brand_tray_template.py` and registered in the brand icon manifest.
- feat: the last reply of an idle conversation has a Regenerate action. It cuts the thread back to before the last message and sends the same text and attachments again, so the answer comes from a normal run; it is not offered while a run is in progress or when the last message was a skill command.

- fix: the settings screen showed the HTML entities `&rsquo;` and `&amp;` literally ("Auto follows your Mac&rsquo;s appearance", "Setup &amp; updates"); the hints now use the characters themselves, and the appearance hint no longer names a single operating system.
- feat: the desktop app moves to the Delta visual system v1: teal is the only accent (a lifted teal in dark mode keeps text at 4.5:1 and the focus ring above 3:1), surfaces are flat with no frosted glass and one shadow for floating layers, keyboard focus has one ring, and status is shown by shape and word instead of colour alone. The retired gray-blue accent is gone, and the old colour names keep working as aliases while components migrate.
- feat: the run status bar sits directly above the composer and shows the running state with an icon and a word. It now follows the dark theme (it stayed light gray before) and honours reduced motion. Paused, resumed and verified states are not shown until the runtime reports them.
- feat [security]: approvals for actions that leave the computer (chat messages, files, connector calls), live and parked in the Inbox, carry a labelled "Needs your approval" band and stay locked until the user confirms the recipient and content; the session-wide "Always allow" is no longer offered for them, and the task-scoped "Allow every time" of an automation run needs the same confirmation. This is a client-side safeguard: the runtime's approval decisions and authority are unchanged. The primary approval button is now a solid fill.
- feat: task steps, the sidebar working marker, connector Live and Ready chips and the voice Ready chip use icons with accessible names, Stop is an inverse button, and error colours are used for errors again instead of the accent.
- feat: the sidebar groups Recent by date (Today, Yesterday, Previous 7 days, Earlier) and lets tasks be filed into collapsible projects that can be created, renamed, cleared and removed (removing a project never deletes tasks). Projects are stored on this device only; the runtime has no project concept and no contract changes.
- feat: new shortcuts: Ctrl/Cmd+K opens search, Ctrl/Cmd+N starts a new task, Ctrl/Cmd+Shift+L switches between light and dark; a collapsed "Thought process" shows a one-line summary of the reasoning.

- fix: the SQLite authority stores (`run_events.db`, `side-effects.db`, `approvals.db`, `tasks.db`, `core.db`, `memory.db`) and `inbox.json` are now owner-only from the moment they are created or opened, like the JSON credential files. `private_fs.rs` gains `harden_sqlite_files(path)` covering the database file and its `-wal`/`-shm` sidecars; on Windows it also restricts the containing directory so sidecars SQLite creates later inherit the restriction.

- build [security]: update the Tauri family together: `tauri` 2.12.1, `tauri-build` 2.7.1, the dialog, autostart, opener and updater plugins (updater 2.12.0 to 2.13.1) and, with them, the `@tauri-apps/api`, `@tauri-apps/plugin-opener` and `@tauri-apps/cli` packages (all resolved from `registry.npmjs.org`), instead of merging the single Dependabot updates whose Rust and JavaScript halves must match. The update also drops the rust-unic crates, so their five advisory ignores are removed from `deny.toml`.

- docs: add ADR-0052 (proposed) on credential storage: where API keys and connector credentials are kept today (Rust authority files, Python vault), the Windows gap in the Rust private write, and the options and recommendation, awaiting the owner's decision; nothing is implemented.
- chore: drop the ten ignore entries for the gtk-rs GTK3 advisories (RUSTSEC-2024-0411 to RUSTSEC-2024-0420) from `deny.toml`; the advisory database withdrew them on 2026-08-14 and `cargo deny check advisories` passes in every workspace without them.
- fix [security]: credential files are now owner-only from creation. On Windows the Rust runtime restricts `model-authority.json`, `application.json` and the MCP state file to the current user's SID (inheritance removed) before the file takes its final name and verifies the result, instead of relying on the folder's access; the Python vault creates files with mode 0600 (`O_EXCL`), compares Windows ACLs by SID instead of English account names, no longer hides a failed folder restriction, and reports `acl_unprotected` in its status rows; the model settings carry `secrets_file_protected`. No data format change (ADR-0052, step 1).

- build: add `packaging/portable/sign-windows.ps1`, an Authenticode signing step that signs the launcher and the app executable before the portable ZIP is built and re-verifies them in the final ZIP, but only when `WINDOWS_SIGNING_CERT` and `WINDOWS_SIGNING_PASSWORD` are present; without them the build logs that signing is skipped and behaves exactly as before.
- fix [security, migration]: the audit database and the run event ledger no longer keep tool arguments and results in clear text. A new `delta_core::redact` module applies the policy of `packages/sanitize.py` (secret-shaped keys, credential headers, `body`/`content`/`html` fields, URL credential parameters) and also scrubs shell command text and tool results for `Authorization:`/`Cookie:` headers, `--password`/`--token` style flags, `?token=` parameters and well-known token shapes (`sk-`, `ghp_`, `AKIA`, JWT). It runs on every audit row written and on every tool event the runtime appends to the ledger; tool names, stages, statuses, levels, resources and the shape of the arguments stay. Audit rows written by older builds are scrubbed once on the next start (`PRAGMA user_version`; a failure never blocks startup). Checkpoints, which hold what is needed to resume a run, are deliberately unchanged, and ledger events written before this change are not rewritten because they are hash chained. Python and Rust are tested against one shared fixture, `crates/delta-core/tests/fixtures/redaction_golden.json`.
- fix [security]: "Open" on an artifact only hands documents, text and common images (`pdf`, `docx`, `xlsx`, `pptx`, `txt`, `md`, `csv`, `png`, `jpg`, `jpeg`, `gif`, `webp`) to the operating system's default application; any other file, including files with no extension, scripts, shortcuts, `.svg`, `.html` and names such as `report.pdf.exe`, is only revealed in the file manager, and the result carries `downgraded: true`. Previously a model-written `.bat` or `.lnk` could be launched by one click.
- fix [security]: `web_fetch` streams the page and stops at 5 MB after decompression or after 30 seconds, whichever comes first, instead of reading the whole body into memory; a response that is cut short is reported with `truncated: true`. Previously a server that never stopped sending, or a small compressed file that expands to gigabytes, could exhaust the memory of the Delta process. Address checks and pinning on every redirect hop are unchanged (`guard.stream_checked`).
- fix [security]: the address guard behind `web_fetch` and `browser_read_url` refuses URLs that carry credentials or a backslash in the host part (`http://127.0.0.1@example.com/` was read as example.com by the guard and as 127.0.0.1 by a browser, and `http://user:pw@host/` was passed through to the pinned request), and it judges IPv4 addresses carried by IPv6 (IPv4-mapped, 6to4, Teredo, NAT64) itself instead of relying on the interpreter's address tables. The module comment no longer says `web_fetch` skips approval.

- fix: the Windows portable build invokes the relocatability scanner by its actual filename `scan-portable-paths.ps1` instead of the mistyped `scan_portable_paths.ps1`, and the BUILD.txt provenance label now names `build-portable.ps1` correctly.

- fix: retired `fongap/delta` references are repaired — the desktop updater feed and portable provenance point to `fongap-labs/delta`, and governance docs use the `fongap-labs` organization path.

- build [security]: upgrade the dev-only `urllib3` from 2.7.0 to 2.8.0 for published advisories, and resolve every desktop npm package from `registry.npmjs.org` instead of a third-party mirror (integrity hashes unchanged).

- ci: add Rust to the CodeQL scan so the Rust core runtime (`delta-core`) is covered by static analysis, not only the TypeScript and Python code.

- fix [security]: the desktop CSP no longer allows inline scripts (`script-src 'self'`); the pre-paint theme script moved from `index.html` into `public/theme-init.js`, and `tauri dev` keeps the previous policy through `devCsp` so Vite's dev preamble still runs.

- fix: the README install command no longer asks for a non-existent `messaging` extra (`uv sync --locked --extra dev` is the command CI runs).

- fix: `read_file_lines` capped its window with the per-line character limit by coincidence; it now uses its own `_READ_FILE_LINES_HARD_MAX` (same value, 500).

- ci: the desktop contract-test list is declared once in `central-ci.sh`, every entry must exist, and the two stale entries (`src/api.contract.test.ts`, `src/api.auth.test.ts`) are gone.

- docs: add `docs/architecture/adr/README.md`, an index of the ADR numbers cited in code but stored elsewhere, and point the Rust doc comments at it instead of at files that do not exist.

- ci: add the approved thin PR dispatcher so pull request events reach central governance within about a minute; it runs no PR code and skips Dependabot.

- test: run the Python contract and integration suites from the central Action Worker test pack (`tests/packs/delta`); Rust and desktop tests stay beside their code.

- fix: truncate provider error bodies on a UTF-8 character boundary instead of a raw byte slice and restore the missing PathBuf test import in the capability contract tests.

- fix: warm up the Windows PowerShell persistent-shell driver before the first real command so cold CI runners do not time out on the first `[Console]::In.ReadLine()` round-trip.

- refactor: move `delta-core` runtime tests into a dedicated submodule so `runtime.rs` contains only production runtime code.

- refactor [breaking]: remove the retired JSONL `delta_core` subprocess server/client path and keep the embedded Rust runtime as the only product Runtime authority.

- refactor: make Foundation connectors consume an injected read-only `SecretSource`, remove dead secret-backed tool enablement state, and keep the concrete file vault only at legacy/headless boundaries.

- feat: add a canonical persistent Worker SDK loop so long-lived workers share the same Job, secret-frame, ABI negotiation, and terminal-result contract as one-shot workers.

- fix: add the packaged Delta Worker SDK README required by Git subdirectory installs.

- fix: make the Python Worker SDK fail closed with typed results for malformed jobs, malformed secret payloads, and invalid handler returns.

- refactor: remove the retired parallel Worker SDK and keep `delta-worker-sdk` as the single Python Capability ABI implementation.

- fix: align Python worker SDK grants, boundaries, input files, and artifacts with the Rust Capability ABI.

- fix: align the Python worker SDK result contract with the Capability ABI so any JSON value can be returned.

- fix: expose a Foundation-owned browser connection address contract so interactive providers can pin connections to the vetted IP and close DNS rebinding gaps.

- ci: cancel centralized PR work when a pull request is closed.

- chore: remove the obsolete Dependabot auto-merge workflow because repository auto-merge is centrally disabled.

- fix: pin address-checked connector HTTP requests to vetted public IPs and preserve request credentials without following redirects.

- perf: run independent Python and Rust central-CI checks with bounded parallelism to reduce full-suite wall time without reducing coverage.

- refactor: move Foundation and Rust license policy checks into Action Worker Central CI and remove the repository-local workflow.

- refactor [breaking]: move Delta main CI, Windows Portable release runners, SBOM packaging, attestation, and Release Governance dispatch to Action Worker.

- refactor [breaking]: bind release artifacts to explicit source and workflow-run provenance for centralized Release Governance.

- refactor: move pull-request CI to Action Worker while retaining main-branch validation until release source/build execution is centralized.

- ci: add a lightweight GitHub Actions merge gate for centralized Action Worker evidence.

- ci: add Linux and Windows project entrypoints for Action Worker centralized CI.

- ci: centralize final merge validation through Action Worker while keeping repository-native checks local.
- fix: make Dependabot uv.lock synchronization follow the current repository instead of a retired owner hardcode.

- fix: refresh model-visible tool schemas for reused session runtimes after Connector state changes.

- refactor [breaking, migration]: hard cut the central governance dispatch credential and derive its repository target.

- fix: harden Windows persistent-shell trailer parsing against prefixed PowerShell output.

## [0.1.0] - 2026-09-18

Initial Delta release.

### Included

- Rust-authoritative Runtime, Trust, state, approval, recovery, and capability execution.
- Tauri + React desktop application with direct Rust IPC.
- Workspace-scoped native and worker capabilities with typed manifests and grants.
- MCP client runtime with stdio and HTTP transports.
- Extension manifests for isolated optional capabilities and connector catalogs.
- Source, citation, artifact, validation, ledger, automation, and memory foundations.
- Versioned Rust and Python worker contracts at the 0.1.0 baseline.
- Repository governance and authority cleanup aligned to the initial 0.1.0 release surface.
