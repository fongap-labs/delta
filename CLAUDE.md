# Agent Guide

This repository inherits Fongap Labs shared governance from [fongap-labs/action-worker](https://github.com/fongap-labs/action-worker/tree/main/docs).

## Required reading

1. `CLAUDE.md`
2. Action Worker `docs/01_SHARED_GOVERNANCE.md`
3. Action Worker `docs/03_NAMING_CONVENTIONS.md`
4. Action Worker `docs/04_CHANGELOG_CONVENTIONS.md`
5. Action Worker `docs/DEVELOPMENT_GUIDE.md`
6. The project-specific documents below

## Project authority

- `docs/DELTA_BLUEPRINT.md`
- `docs/architecture/target-architecture.md`
- `docs/architecture/runtime-public-contract.md`
- `docs/architecture/capability-abi.md`
- `docs/governance/REPOSITORY_BOUNDARY.md`
- `docs/governance/CONTRACT_GOVERNANCE.md`

## Project-specific rules

- Delta Foundation owns the single authority plane and runtime.
- Optional extensions use public contracts and must not depend on Foundation internals.
- TypeScript owns product interaction; Rust owns runtime and authority; workers remain controlled execution surfaces.

## Change conventions

The PR Gate enforces these rules. Read the linked shared sections before opening a pull request; they are not repeated here.

- PR titles and engineering text (code comments, logs, test descriptions, configuration keys) are English. See `01_SHARED_GOVERNANCE.md` (language table) and `04_CHANGELOG_CONVENTIONS.md` §3.
- Changelog entries go in a new fragment under `changelog.d/` (`04_CHANGELOG_CONVENTIONS.md` §4–§5). A pull request must not edit `CHANGELOG.md` `[Unreleased]`.
- Human-facing documents (README, guides, governance notes) are Chinese.

## Precedence

```text
Action Worker machine contracts / shared governance
        ↓
this repository's project architecture / project boundary
        ↓
implementation documentation
        ↓
README / examples
```

Shared governance is not duplicated here. Project documents may add stricter product-specific constraints but must not bypass the shared trust, PR Gate, release or repository-governance contracts.
