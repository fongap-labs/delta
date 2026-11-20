# Delta Foundation Contract Rules

This file is the machine-read authority for the rules and data shapes in `contracts/`. The human-facing guide is `README.md` (Chinese). The rules are not repeated there.

Stable, versioned contracts define the boundary between Delta Foundation and Delta Suite. Contract is the **only** extension boundary.

## Capability ABI

See `docs/architecture/capability-abi.md` for the authoritative Capability ABI specification. The data shapes below are the canonical, versioned contract types.

## Execution Grant

Bound a capability invocation to a run, an execution epoch, a permission scope, and an expiry. Grants prevent stale jobs from continuing and prevent cross-scope or out-of-epoch calls. Timestamps are Unix epoch seconds (f64).

```json
{
  "run_id": "run_...",
  "execution_epoch": 1715932000.0,
  "capability_id": "research.statistics.anova",
  "permission_scope": ["filesystem.read"],
  "expires_at": 1715935600.0
}
```

See `schemas/execution-grant.schema.json`.

## Capability Request

```json
{
  "run_id": "run_...",
  "tool_call_id": "tool_...",
  "capability": "research.statistics.anova",
  "deadline": "2026-09-17T12:00:00Z",
  "workspace": { "id": "ws_..." },
  "inputs": [],
  "permissions": [],
  "limits": {}
}
```

## Contract stability rules

1. Breaking changes require a version bump and a contract test.
2. Foundation never imports Suite contract impls.
3. Suite consumes Foundation contracts only through the Delta SDK crate
   (`crates/delta-sdk`), the public Python extension API (`delta_extension_api`,
   `delta_worker_sdk`), and the manifests installed under
   `<state>/extensions/<source>/`. Python entry points are not an activation
   mechanism (see `docs/architecture/extension-loading.md`).
4. New contract types must be added to `schemas/` as JSON Schema before use
   across a process boundary.
