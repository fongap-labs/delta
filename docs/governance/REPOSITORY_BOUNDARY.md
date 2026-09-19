# Delta Extension Boundary

Version: 2.0

## Purpose

Delta Foundation is a complete, independently runnable product. Optional
extensions may add capabilities through stable public contracts, but Foundation
must never require an extension implementation in order to build, test, start,
or perform its baseline work.

## Dependency direction

The only supported direction is:

```text
Optional extension
       ↓
Delta public SDK / extension API / contracts
       ↓
Delta Foundation
```

Forbidden:

```text
Delta Foundation → extension implementation
Extension → Foundation internals
```

## Foundation responsibilities

Foundation owns:

- the App and Core runtime;
- Trust, Permission, Approval, State, Recovery and Ledger authority;
- public SDKs, schemas and cross-process contracts;
- CapabilityHost and Worker supervision;
- generic baseline capabilities;
- extension discovery, validation and isolation.

Foundation must not encode optional extension identifiers, implementation logic,
vendor-specific workflows, or entitlement decisions in Core authority logic.

## Extension responsibilities

An extension may provide:

- capabilities and workers;
- connectors;
- skills and workflows;
- optional managed-service adapters;
- capability-specific presentation metadata.

An extension must not create a second App, Core, Runtime, Trust model, durable
authority store, or approval path.

## Runtime installation contract

Each installed extension owns a directory under:

```text
<delta-state>/extensions/<source>/
```

Its manifests are isolated from other extensions. Foundation discovers and
validates them independently. A broken optional extension must not prevent
Foundation or unrelated extensions from starting.

## Compatibility

Extensions target released public contracts rather than Foundation main or
internal implementation paths. Breaking public-contract changes require a
versioned compatibility boundary.

## Hard rules

1. Foundation builds, tests and runs with no optional extensions installed.
2. Foundation never imports optional extension source.
3. Extensions use only public SDK / extension API / contracts.
4. Every extension is independently installable, removable and diagnosable.
5. Runtime authority remains in Foundation.
6. CI enforces these rules.
