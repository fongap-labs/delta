# Architecture Decision Records

## In this repository

| ADR | Title |
|---|---|
| [0001](0001-one-delta-one-core.md) | One Delta, one core |
| [0002](0002-foundation-suite-boundary.md) | Foundation / extension boundary (historical file name) |
| [0003](0003-capability-over-fork.md) | Capability over fork |
| [0004](0004-capability-execution-chain.md) | Capability execution chain |
| [0052](0052-credential-storage.md) | Credential storage (proposed; numbered 0052 so it does not collide with the earlier ADR-005 ... ADR-051 numbers cited in code) |
| [0053](0053-resume-run-identity.md) | Which run identity a resumed run uses (proposed) |

## Numbers cited in code but not stored here

Source comments and docs also cite ADR numbers from an earlier numbering scheme
(`ADR-005` ... `ADR-051`). Those decision documents are **not part of this repository**;
the table only records what the citing code says each number covers, so a contributor
knows what a comment refers to. Treat the code and `docs/architecture/runtime-public-contract.md`
as the source of truth. When one of these decisions is written up in the current
`NNNN-title.md` format, add it to the first table and drop it from this one.

| ADR | Topic, as cited in code | Cited in |
|---|---|---|
| ADR-004 | Managed OAuth removed; manual connect | `apps/desktop/src` |
| ADR-005 | Reliable task runtime (incl. WS3: Validation) | `crates/delta-core/src/artifact.rs`, `validation.rs` |
| ADR-009 | Delta core architecture | `crates/delta-core/src/lib.rs` |
| ADR-019 | R2 pre-plumbing; artifact registry shadow reader, validation shadow validator | `artifact.rs`, `validation.rs` |
| ADR-020 | Rust write authority for the artifact registry | `artifact.rs` |
| ADR-022 | Rust-authoritative tool idempotency state machine | `tool_lifecycle.rs` |
| ADR-024 | Hard cut: Rust is the sole authority for the task store | `taskstore.rs` |
| ADR-027 | R2 source / citation hard cut | `source_citation.rs` |
| ADR-028 | Write authority for validation trusted facts | `validation.rs` |
| ADR-029 | Checkpoint authority (`checkpoint.registered` events) | `lib.rs` |
| ADR-030 | Policy authority (tool-call risk levels, policy slices) | `lib.rs` |
| ADR-031 | Approval authority (approval audit events) | `lib.rs` |
| ADR-037 | Cancellation decision authority | `tool_lifecycle.rs` |
| ADR-038 | Timeout decision authority | `tool_lifecycle.rs` |
| ADR-039 | Retry policy decision authority | `retry.rs` |
| ADR-042 | Run lifecycle state machine and checkpoints | `ledger.rs`, `runtime.rs` |
| ADR-044 | `next_run` computed by pure Python `compute_next_run()` | `taskstore.rs` |
| ADR-051 | Rust core control-plane authority | `lib.rs` |
