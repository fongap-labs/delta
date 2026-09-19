# Delta Foundation AI Agent Instructions

## Scope

These rules apply to changes in the public Delta Foundation repository.

## Boundary

Foundation must remain complete and independently buildable, testable and
runnable with no optional extensions installed.

Allowed dependency direction:

```text
Optional extension
       ↓
Public Delta extension API / SDK / contracts
       ↓
Delta Foundation
```

Foundation must not depend on an optional extension implementation, private
registry, private package, or extension-specific credential.

## Authority

Do not create a second:

- App
- Core
- Runtime
- Trust/Permission/Approval authority
- durable State/Ledger authority

Optional workers execute below Foundation authority through CapabilityHost and
versioned contracts.

## Extension implementation rule

Foundation owns generic contracts and baseline implementations. Vendor-specific
or product-specific implementations should enter through public extension
surfaces rather than being compiled into Core.

Installed manifests live under:

```text
<delta-state>/extensions/<source>/
```

Never introduce a single global extension manifest that lets one package
overwrite another.

## Change rule

Before changing a public extension API:

1. identify existing consumers;
2. preserve compatibility or version the contract;
3. add/adjust contract tests;
4. keep Foundation runnable without consumers present.

## Repository hygiene

Developer-facing source, docs, CI and commit messages are English-first.
Do not commit credentials, machine-local paths, migration snapshots, or private
implementation details.
