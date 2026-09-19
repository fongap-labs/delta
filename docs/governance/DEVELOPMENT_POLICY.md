# Development Governance

This document defines Delta's day-to-day development rules, architecture boundaries, naming, documentation, and migration governance.

The long-term product boundary is defined by `docs/DELTA_BLUEPRINT.md`. Target architecture is defined by `docs/architecture/target-architecture.md` and the active ADR set. Current execution plans remain implementation guidance, not a second architecture authority.

## 1. Long-term standard

Architecture and development decisions should optimize for:

```text
Long-term | Stable | Efficient | Secure | Agile
```

Do not introduce complexity merely for architectural symmetry, language purity, branding uniformity, or directory aesthetics.

## 2. Product and module classification

Primary product domains are limited to:

```text
Everyday productivity
Research and analysis
Content creation
```

Long-term logical responsibilities are:

```text
Experience
Runtime
Trust
Work
Capability
Automation
Learning
```

These are responsibility boundaries. They do not require one crate, package, or service per responsibility.

Preferred order for adding capabilities:

```text
compose existing Capability
→ Skill
→ new Capability / Worker
→ extend Rust Host / Runtime only when necessary
→ new Agent / persistent service as a last resort
```

## 3. Naming governance

> **Use `delta-*` for real independent boundaries; use semantic names for internal responsibilities. Branding identifies product boundaries, not every internal module.**

Use `delta-*` / `@delta/*` only when there is a genuine independent boundary, such as:

- an executable;
- a crate or package with a public API;
- a version compatibility boundary;
- a cross-language protocol;
- an SDK;
- an independent release or distribution unit.

Recommended examples:

```text
Delta / Delta.exe
delta-runtime
delta-protocol
delta-capability
delta-worker
delta-worker-py    # only if a real Python SDK boundary exists
delta-cli          # only if an independent CLI remains
delta-testkit      # only if a stable public testkit exists
@delta/*           # only for real TS workspace package boundaries
```

Internal responsibilities should keep semantic names such as:

```text
session
run
context
provider
policy
approval
ledger
source
artifact
validation
memory
experience
skill
```

Do not create branded wrappers such as `delta-trust`, `delta-work`, `delta-learning`, or `delta-artifact` unless they become real independent release, version, protocol, process, or SDK boundaries.

Detailed naming rules belong in `docs/architecture/repository-layout.md`.

## 4. Language and Authority

Core product language target:

```text
Rust + TypeScript
```

- TypeScript: Experience and UI.
- Rust: Runtime, Trust, Work, Automation, Learning Authority, and Capability Host.
- Python / PowerShell / Shell: controlled Worker / Script environments.

Workers must not own Session, Run State, Policy / Approval, Core DB, Artifact formal state, Secrets Authority, or the model/provider control plane.

A domain has one Authority. When an Authority migration is complete, remove the previous owner or forwarding path rather than maintaining dual control indefinitely.

## 5. Repository language

The source repository is English-first.

Use English for:

- README and public documentation;
- architecture, governance, ADRs, and contracts;
- source-code comments and docstrings;
- commit messages and PR titles;
- CI job names, scripts, and developer-facing errors where practical.

User-facing localization is separate. Delta may provide Chinese and other UI languages without making repository governance multilingual by default.

Existing non-English documents may be converted progressively when they are touched. Avoid translation-only churn when it does not improve an active document.

## 6. Runtime and Authority work

Runtime and Authority changes must state:

```text
Authority Before
Authority After
Compatibility
Exit Condition
Failure / Rollback
Tests
```

Control-plane migration is about ownership and process topology, not rewriting every Office, Statistics, or Media capability for language purity.

## 7. Research analysis and Learning

Statistics, DOE, and sequential studies must distinguish:

- deterministic calculation;
- method selection;
- research hypothesis;
- adjustable scope;
- stopping rule;
- user decision.

Sequential design must not silently rewrite the research objective or stopping criteria after observing interim results.

Learning may improve Preferences, Experience, Skills, Workflows, and Templates. It must not automatically lower Approval requirements, increase Risk grants, widen Network / Secrets / File scope, or change Core Authority.

## 8. Branches and commits

Normal development branches from `main` and enters `main` through Pull Requests.

Recommended branch prefixes:

```text
feat/*
fix/*
refactor/*
ci/*
chore/*
docs/*
test/*
release/*
```

Recommended commit format:

```text
<type>: <description>
```

Commit messages, comments, PR titles, and active documentation must describe the current Delta change directly. Do not use old-repository snapshot, import, or migration-source wording as the identity of current work.

Default merge strategy is Squash merge.

## 9. Pull Requests

A PR should state at least:

- objective;
- product domain / logical responsibility;
- major changes;
- validation;
- compatibility;
- security / permission impact;
- documentation impact;
- whether it meets CHANGELOG criteria.

Review should verify:

1. the change solves a real goal;
2. it serves one of the product domains;
3. Capability / Skill is preferred where appropriate;
4. unnecessary complexity is avoided;
5. Authority remains clear;
6. no second Core Authority is introduced;
7. Worker / Skill / Learning cannot bypass Trust;
8. naming follows real independent boundaries;
9. tests and documentation remain aligned.

## 10. Documentation governance

Active documents have one clear responsibility:

- `DELTA_BLUEPRINT.md`: long-term product boundary;
- `target-architecture.md`: long-term system target;
- `repository-layout.md`: current physical structure and naming;
- `runtime-public-contract.md`: stable Runtime / Human Control contract;
- `capability-abi.md`: Capability / Worker boundary;
- active execution plans: current implementation state and exit conditions;
- ADRs: architecture decisions;
- CHANGELOG: release-level changes;
- audits: audit evidence;
- operations: platform operations.

One-time migration notes that no longer define the current state should not remain in active architecture or product documentation. Historical details belong in Git, ADRs, changelogs, or review records when materially useful.

Do not create `latest`, `final`, `v2`, or `new` copies of active governance documents.

## 11. CHANGELOG

Record user-visible changes and important compatibility, Runtime, Security, and Release changes. Do not record every commit, debugging step, test count, or ordinary internal refactor.

Released versions are normally immutable.

## 12. Dependencies and release

Dependency rules belong in `dependency-policy.md`, quality gates in `quality-policy.md`, and release rules in `release-policy.md`.

New dependencies must clearly belong either to the Rust / TypeScript Core product or to a controlled Worker ecosystem. They must not recreate a secondary control plane.
