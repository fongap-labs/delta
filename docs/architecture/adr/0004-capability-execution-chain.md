# ADR-0004: Capability Standard Execution Chain

Status: Accepted

## Context

R8.3 requires that all capability invocations follow a single, enforceable
execution chain. No capability — native, worker, or adapter — may bypass
Trust, Grant, Validation, or Ledger.

## Decision

The standard execution chain is:

```
Capability requested
    ↓
Registry lookup
    ↓
Manifest / requirements
    ↓
Trust / Policy evaluation
    ↓
Approval if required
    ↓
Scoped Execution Grant
    ↓
Execution validation (R8.1)
    ↓
Dispatch
    ↓
Native / Worker / Adapter
    ↓
Structured result
    ↓
Validation
    ↓
Formal Artifact
    ↓
Ledger
```

### Enforcement points

1. **Registry lookup** — `CapabilityRegistry::get()` is the sole entry
   point for tool dispatch. Unregistered tools are rejected.

2. **Trust / Policy** — `RuntimeHost::execute_tool_call()` calls
   `policy::evaluate()` before any dispatch. Policy denial prevents
   execution; approval is required for risk levels that need user
   confirmation. No code path reaches `CapabilityHost::execute()`
   without passing through policy.

3. **Execution Grant** — `CapabilityHost::execute()` calls
   `CapabilityBoundary::validate_execution_grant()` (R8.1) before
   dispatching to any Runner. Invalid grants never reach a Runner.
   `WorkerProcessRunner` adds a defensive secondary check.

4. **Dispatch** — Only `CapabilityHost` implements `ToolExecutor`. The
   `RuntimeHost` owns the `tool_executor` field and calls it exclusively
   from `execute_tool_call()`, which is the sole call site.

5. **Artifact formalization** — Workers stage artifacts to
   `artifact_staging_dir`. The Runtime promotes staged artifacts via
   hashing, validation, and `ArtifactRegistryWriter` registration.
   Workers never register artifacts directly.

6. **Ledger** — Every stage (proposed, approved, started, progress,
   completed/failed) is persisted to the ledger. The ledger is the
   audit trail.

### Bypass prevention

- **Capability cannot bypass Trust**: `CapabilityHost::execute()` is
  only callable through the `ToolExecutor` trait, which is only invoked
  by `RuntimeHost::execute_tool_call()` after policy evaluation.
- **Worker cannot bypass Grant**: R8.1 grant validation in
  `CapabilityHost` + defensive check in `WorkerProcessRunner`.
- **Adapter cannot bypass Policy**: MCP adapters (`McpCapabilityRunner`)
  wrap `WorkerProcessRunner`, which is dispatched by `CapabilityHost`,
  which is gated by policy in `RuntimeHost`.
- **MCP is not a Runtime**: `McpCapabilityRunner` is a `CapabilityRunner`
  adapter, not a separate runtime. It delegates to `WorkerProcessRunner`.
- **Connector does not create formal Artifacts**: Connectors produce
  staged output; the Runtime formalizes it.
- **Skill does not bypass Capability boundary**: Skills declare required
  capabilities; they do not invoke runners directly.

## Consequences

- No new top-level "Resolver service" is created. Registry lookup is an
  internal responsibility of `CapabilityHost`.
- The chain is enforced structurally, not by convention.
- Adding a new capability type (native, worker, adapter) requires
  implementing `CapabilityRunner`, which is dispatched by
  `CapabilityHost`, which is gated by the full chain.
