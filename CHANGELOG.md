# Changelog

## [Unreleased]

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
