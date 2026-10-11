# ADR-0054: Baseline work capabilities live in Foundation

Status: **Accepted** (owner, 2026-10-11). Implementation follows in steps; each step is listed in the CHANGELOG when it lands.

## Context

The blueprint names three kinds of work: daily office, research and content. ADR-0002 and `REPOSITORY_BOUNDARY.md` say Foundation must run and do "baseline work" with no extension installed, and that specialist capabilities are optional extensions.

What Foundation does today (checked in `CapabilityHost::product_defaults`, `crates/delta-core/src/capability/host.rs`, and its test `product_registry_owns_stable_model_tool_schemas`): the model gets five tools, `ask_user`, `propose_plan`, `read_file`, `request_directory` and `write_file`. It cannot read a PDF, Word or Excel file, fetch a web page, search the web or create an Office file. The Python versions of these tools exist under `integrations/` but the desktop app does not ship Python and the Rust runtime does not call them, so they are not available in the product.

So a user who installs Foundation alone cannot do the first thing the blueprint promises: read a document, research a question, produce a deliverable. That conflicts with "Foundation is a complete, independently runnable product".

## Options

**A. Treat reading and writing common work files and reading the web as baseline capabilities, implemented natively in Foundation (chosen).**
- Plus: works after install, no interpreter, one Trust path, nothing to install or keep in step.
- Minus: Foundation gains parser/writer dependencies and network code that must be reviewed; the blueprint's "no heavy Office suite" limit must be kept by scope, not by location.

**B. Keep them as extensions, shipped as Python workers.**
- Plus: matches the earlier reading of ADR-0002 literally.
- Minus: the product has no Python, so the baseline stays empty until a user installs a runtime and an extension.

## Decision: A, with a narrow definition of "baseline"

Foundation provides these as native capabilities under the Capability Host, with the same grants, approvals, ledger and artifact rules as `read_file` and `write_file`:

| Tool | What it does | Risk and approval |
|---|---|---|
| `read_document` | Text and structure out of PDF, DOCX, XLSX and PPTX files in the workspace | low, no approval (reads like `read_file`; counts as a source read) |
| `web_fetch` | One page, as readable text | medium, approval (egress; the URL is model-chosen) |
| `web_search` | Titles, URLs and snippets for a query | medium, approval (egress; the query is model-chosen) |
| `create_document` | A new DOCX, XLSX or PPTX from structured content, checked by reading it back | medium, approval, staged as an artifact like `write_file` |

Rules that keep this inside the blueprint:

1. **Scope.** Reading, and creating files from structured content. Not editing in place, layout design, macros, or an Office-compatible editor (blueprint 2.1: "not a full Office suite").
2. **Network.** `web_fetch` and `web_search` refuse loopback, private, link-local (including the cloud metadata address), CGNAT, multicast and reserved addresses, including IPv4 carried inside IPv6, on every redirect hop, and connect only to the address that passed the check (so a name that changes its answer cannot redirect the connection). They cap the body at 5 MB after decompression and the read at 30 seconds, and return external text as data, never as instructions. This is the policy of the Python `integrations/web/guard.py`, ported, not loosened.
3. **Validation.** `create_document` reads the file back with the same parser as `read_document` and fails if the structure it was asked for is not there. A document is never reported as created on the model's say-so (blueprint 4.4).
4. **Dependencies.** Pure Rust only, each covered by the repository's cargo-deny policy, each serving the Work and Capability modules: a ZIP/XML reader (`zip`, `quick-xml`) and a PDF text reader (`lopdf`); the DOCX, XLSX and PPTX writers build the packages directly on `zip`, with no further dependency. `pdf-extract` and older `lopdf` releases were rejected because they pull in `ttf-parser`, which the advisory database lists as unmaintained and `deny.toml` does not allow. No system libraries, no interpreter.
5. **Extensions remain.** Specialist capabilities (statistics and experiment design, image and video, connectors) stay optional extensions. An extension may replace a baseline tool by registering a tool under another name; Foundation names no extension.

## Consequences

- `REPOSITORY_BOUNDARY.md` needs no change: "generic baseline capabilities" now has a stated meaning in this ADR.
- The Python `integrations/tools` and `integrations/web` code is the behavioural reference for the ports. It is removed only when nothing calls it, as `DEPENDENCY_POLICY.md` requires.
- New tools need no new approval path: `web_fetch` and `web_search` are already classified as egress in `policy.rs`.
- Search is a user-ordered list of engines: Google, Bing and DuckDuckGo built in without a key, Brave and Tavily with one, and engines the user adds (a URL with `%s`). The list and every key are one credential-store value granted only to the search tool.

## Not verified

The generated DOCX, XLSX and PPTX files were opened with python-docx, openpyxl and python-pptx and rendered with LibreOffice, not with Microsoft Office. PDF text comes out one text run per line.

Page and search-result parsing was written against the markup seen at the time and will need upkeep as providers change it; the search parsers are tested against saved samples, not against the live sites.
