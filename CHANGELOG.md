# Changelog

## [Unreleased]

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
