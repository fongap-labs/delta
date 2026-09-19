# Provenance & Third-Party Governance

Delta is an independent project. It does not maintain a mandatory upstream, mirror branch, or automatic synchronization relationship with any external project.

This document governs source provenance, third-party code adoption, and license obligations. It does not duplicate product scope, target architecture, or Runtime design.

## Principles

- External projects are references, not product authorities.
- External code, behavior, or architectural ideas must be independently evaluated before entering `main`.
- Delta does not restore retired directories, protocols, Coding-oriented product shapes, compatibility layers, or multi-Agent structures merely to follow another project.
- Active documentation describes the current Delta architecture. Historical details belong in Git, ADRs, changelogs, audit records, and legal notices when materially useful.
- Third-party copyright and license obligations follow the code and assets that remain in the repository, regardless of repository history.
- Third-party names must not be used as current Delta product, architecture, module, Runtime, or release identities.

## Evaluation filter

Any external implementation considered for Delta must answer:

1. Does it improve a real workflow in productivity, research, or content creation?
2. Which responsibility owns it: Experience, Runtime, Trust, Work, Capability, Automation, or Learning?
3. Who owns the state and Authority?
4. Can it be implemented as a Capability or Skill before expanding Runtime or Agent scope?
5. Does it fit the Rust + TypeScript Core boundary and controlled Worker model?
6. Does it introduce new security, licensing, or supply-chain risk?
7. Does it have tests, validation, and a rollback path?

## External reference is not upstream identity

Delta may study strong ideas from external projects in Runtime, UI, Trust, Office, Research, Content, Skill, Memory, Automation, and related areas. A reference project does not become Delta's product identity or release authority.

The intended adoption path is:

```text
problem semantics
→ verifiable behavior
→ Delta-owned contract
→ independent implementation or compliant adoption
```

Delta does not inherit external branding, directory structure, or historical baggage by default.

## Naming and provenance

External module names do not automatically enter Delta's naming system.

Delta naming rule:

> **Use `delta-*` for real independent boundaries; use semantic names for internal responsibilities.**

A branded `delta-*` name is appropriate only when there is a genuine independent executable, API, protocol, SDK, version boundary, or release boundary.

External project names are permitted only where materially required for:

- copyright and license attribution;
- third-party notices;
- historical ADRs or audit evidence;
- vulnerability and supply-chain records;
- reproducibility of a provenance decision.

They must not appear in current Runtime configuration, endpoint defaults, user-facing product identity, package naming, active architectural authority, or release naming.

## License boundary

Delta-owned Foundation source is licensed under the Apache License, Version 2.0. The root `LICENSE` applies to Delta-owned work; project attribution is carried in `NOTICE`.

Third-party code and assets are not relicensed by the root Apache-2.0 license. Their attribution is registered in `THIRD_PARTY_NOTICES.md`, and full upstream license texts are stored under `LICENSES/`.

Before introducing third-party code or assets, confirm:

- the original license;
- required copyright notices;
- NOTICE or attribution obligations;
- modification and redistribution rights;
- compatibility with Delta's repository license;
- additional terms for generated code, model weights, media, or other assets.

If the obligation is unclear, do not merge the material.

Repository recreation, history rewriting, renaming, or changing the license on Delta-owned work does not remove third-party obligations from material that remains derived from a third party.

## Current provenance status

Current Delta source is governed by the repository's present license, notices, and
third-party attribution records. Historical implementation audits and superseded
lineage decisions remain available through Git history when needed for legal or
security review; they are not part of the active architecture contract.

## Ownership records

Current product identity is defined by current Delta contracts and governance, not by historical project lineage. Provenance records are retained only where necessary for licensing, security, architecture decisions, or reproducibility.
