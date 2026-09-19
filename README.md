<div align="center">

<img src="resources/brand/delta-logo-256x256.png" width="96" alt="Delta logo">

# Delta

**Local-first AI agent for personal work**

Productivity · Research & analysis · Content creation

[**English**](README.md) · [简体中文](README.zh-CN.md)

[![CI](https://github.com/fongap/delta/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/fongap/delta/actions/workflows/ci.yml)
[![CodeQL](https://github.com/fongap/delta/actions/workflows/codeql.yml/badge.svg?branch=main)](https://github.com/fongap/delta/actions/workflows/codeql.yml)
![License](https://img.shields.io/github/license/fongap/delta?label=License)

[Architecture](docs/architecture/target-architecture.md) · [Extension Boundary](docs/governance/REPOSITORY_BOUNDARY.md) · [Development Policy](docs/governance/DEVELOPMENT_POLICY.md)

</div>

## What is Delta?

Delta is a local-first personal work AI agent for three domains:

- **Productivity** — documents, spreadsheets, PDFs, presentations, messages, files, and routine office work.
- **Research & analysis** — sources, experimental design, data analysis, modeling, visualization, interpretation, and reports.
- **Content creation** — research, writing, editing, images, video materials, platform adaptation, and publishing preparation.

Delta is not designed around accumulating models or agents. It is designed around completing real work while keeping execution **controllable, recoverable, verifiable, and traceable**.

## Why it exists

General-purpose agents can perform useful steps, but long-running work becomes fragile when state, permissions, evidence, side effects, and deliverables are implicit.

Delta makes those boundaries explicit:

```text
Goal
  ↓
Workspace / Session
  ↓
Run
  ↓
Sources / Citations
  ↓
Capabilities
  ↓
Artifacts / Validation
```

The model can decide how to proceed. Delta keeps authority over work state, approvals, recovery, provenance, and validation.

## How it works

```text
Experience
TypeScript / React
       ↓
Runtime
Rust
       ↓
Trust · Work · Capability · Automation · Learning
       ↓
Native tools · Workers · MCP · Connectors · External services
```

Core principles:

- **Local first** — core work state stays local unless a capability explicitly uses an external service.
- **Human control** — high-consequence actions pass through policy and approval boundaries.
- **Recoverable runs** — work can be paused, cancelled, resumed, and inspected.
- **Verifiable artifacts** — sources, citations, versions, provenance, and validation remain part of the work record.
- **Bounded extension** — new domains enter through Skills and Capabilities instead of creating parallel runtimes or authority models.
- **Narrow model boundary** — Delta maintains OpenAI-compatible and Anthropic-compatible interfaces; provider pooling, key rotation, fallback, quotas, and regional routing belong outside Delta Core.

## Extension model

**Skill** is the primary unit for reusable work methods. A Skill may define instructions, workflow, required capabilities, permissions, validation, templates, and optional scripts.

Capabilities may be implemented as native tools, controlled Workers, MCP servers, Connectors, or external adapters.

Optional extensions depend on Delta's public extension contracts. Delta Foundation does not depend on extension implementations, and extensions do not own core Runtime, Trust, Work, or state authority.

See [REPOSITORY_BOUNDARY.md](docs/governance/REPOSITORY_BOUNDARY.md).

## Repository

```text
apps/          User applications
contracts/     Versioned cross-process contracts
core/          Python helpers for controlled capability workers
crates/        Rust workspace crates
integrations/  Capability, Connector, MCP, Skill, and Tool integrations
packages/      Shared foundations and worker helpers
resources/     Brand and product resources
schemas/       Contract schemas
scripts/       Validation and maintenance
tests/         Contract, integration, and cross-language tests
docs/          Architecture and governance
```

The long-term core product boundary is **Rust + TypeScript**. Python, PowerShell, and Shell are controlled execution environments, not core authority layers.

## Development

```bash
uv sync --locked --extra dev --extra messaging
cd apps/desktop
npm install
npm run tauri dev
```

## Release

The `Portable Release` workflow builds and validates the Windows package, generates
`release-manifest.json`, and uploads the immutable `delta-release` artifact. A thin
`workflow_run` dispatcher sends only the source run identity to Action Worker.
Action Worker revalidates the default-branch commit, CI evidence, manifest, assets,
and checksums before publishing the `delta-v<semver>` Release to `external-vault`.
Target-repository credentials are not stored in this repository.

## Documentation

[Blueprint](docs/DELTA_BLUEPRINT.md) · [Target Architecture](docs/architecture/target-architecture.md) · [Runtime Contract](docs/architecture/runtime-public-contract.md) · [Capability ABI](docs/architecture/capability-abi.md) · [Repository Layout](docs/architecture/repository-layout.md) · [Development Policy](docs/governance/DEVELOPMENT_POLICY.md)

English is canonical. [简体中文](README.zh-CN.md) is maintained for Chinese readers.

## License

Delta-owned Foundation source is licensed under the [Apache License 2.0](LICENSE). Third-party code and assets retain their original licenses. See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md), [NOTICE](NOTICE), and [LICENSES/](LICENSES/).
