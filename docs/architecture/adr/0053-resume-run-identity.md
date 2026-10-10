# ADR-0053: Which run identity a resumed run uses

Status: **Proposed**, waits for the owner's acceptance. Nothing is implemented.

## Context

The runtime public contract says (`runtime-public-contract.md`, section 2, Run): "resume keeps the original `run_id`", and invariant 2 says one run identity runs through Ledger, Artifact, Validation, SideEffect and Source. What the code does today (checked, not assumed):

1. `RuntimeHandle::resume` (`crates/delta-core/src/runtime/handle.rs`) generates a **new** `run_id` with `uuid_v4()`, hands it to `RuntimeHost::set_run_id`, and `RuntimeHost::resume` writes `run.started` with `{"kind": "resume"}` under that new id. The interrupted run is not touched. `retry` does the same.
2. The ledger state machine already allows resuming in place: `Ledger::transition` accepts `run.resumed` from `running`, `resumed` and `interrupted` (`ledger.rs`, ADR-042), and `open_runs` treats `resumed` as open. Nothing produces `run.resumed` today.
3. Checkpoints carry both ids (`CheckpointRecord` has `run_id` and `session_id`), and `CheckpointReader::list(None, Some(session_id))` returns a session's checkpoints, so the runtime can find the run a session was last working on without asking the UI.
4. `recover_interrupted_runs` closes runs that were mid-execution at a restart with `run.interrupted`, and keeps runs parked on a human action open.

So the contract and the code disagree. The disagreement shows in more than the ledger: artifacts, validation records, side-effect state, checkpoints and the new `source.read` records (PR #138) are all keyed by `run_id`. After a resume they are split over two ids, so anything that reads "this run" (the trust strip, an audit export) sees only the part after the resume, and the interrupted part looks like a run that ended without a result.

Because of this the desktop does not offer a "Resume" action after "Interrupted"; the status is shown, but the user can only send a new message.

## Options

**A. Keep a new `run_id` and change the contract.** Resume becomes "a new run that continues the session".
- Plus: no code change; resume and retry stay alike.
- Minus: invariant 2 weakens (one logical piece of work has two identities); every consumer that wants "the whole run" must follow a link that does not exist yet; the interrupted run stays closed as interrupted forever.

**B. Resume under the original `run_id` and write `run.resumed` (recommended).**
- Plus: matches the contract and invariant 2; one run keeps one set of events, artifacts, validations, sources and checkpoints; the state machine already allows it; `recover_interrupted_runs` and `open_runs` already treat `resumed` as open.
- Minus: a run can have more than one `run.started`/`run.resumed` segment, so "duration" and "latest event" consumers must not assume one segment; the closed-run rule "interrupted is not re-listed" needs the explicit `resumed` event (it already has it); the runtime must choose which run to resume.

**C. New `run_id` that records `resumed_from: <old id>`.**
- Plus: runs stay immutable once closed; the link is explicit.
- Minus: every "this run" reader has to walk the chain; the same split of artifacts and sources as today, only documented; two ids still describe one piece of work.

## Decision (proposed): B

`resume` keeps the original run identity.

- **Choosing the run.** The runtime picks it, not the UI: the run of the session's latest checkpoint, and only if the ledger says that run's latest lifecycle state is `interrupted`. No such run, or the session already has an active run: reject with an error. It never falls back to starting a new run silently.
- **Event.** Append `run.resumed` (actor `user`, payload `{"kind": "resume", "previous_status": "interrupted"}`) to that run, through `Ledger::transition`. Then continue the loop from the session messages, as today. `run.started` is written once per run.
- **Return value.** `runtime_resume` returns the same `runId` as the interrupted run, which is what the desktop already expects.
- **Side effects.** Tool calls of the interrupted run keep their state. A call that was `Uncertain` stays uncertain and is not replayed (invariant 4); the resumed segment issues new tool call ids, which the idempotency log already keys by `(run_id, tool_call_id)`.
- **`retry`.** Unchanged in this ADR. A retry follows a failed run and starts a fresh attempt, so it keeps a new `run_id`. If that should also be linked to the failed run, it is a separate decision (option C's `resumed_from`, applied to retry only).
- **Contract text.** Section 2 (Run) already says what B does; add the exact event, the choice rule and the "rejects instead of starting a new run" rule. No change to invariants.

## Consequences

- Code: `handle.rs` resume path chooses the run id from the checkpoint reader and the ledger state; `RuntimeHost::resume` calls `transition("run.resumed")` instead of `run.started`; existing tests that assert the new-id behaviour change.
- Tests required: resume of an interrupted run keeps its id and appends `run.resumed` once; resume with no interrupted run is rejected and writes nothing; resume while a run is active is rejected; a closed (completed, failed, cancelled) run cannot be resumed; the hash chain still verifies across the segments; artifacts and `source.read` records written after the resume sit under the original id; an `Uncertain` call before the interruption is still uncertain after.
- Desktop: only after this lands can the UI show "Resume" next to "Interrupted". It would be offered only when the runtime reports the interruption, and the button calls `runtime_resume`; the UI derives nothing (invariant 6).
- Data: no migration. Runs interrupted before this change are resumable the same way, because the checks use data that already exists.

## Questions for the owner

1. Accept B?
2. Should retry also be linked to the failed run (option C for retry only), or stay as it is?
3. Should a run parked on a human action (approval, question) that survived a restart be resumed through the same command, or only by answering the pending decision?

## Not verified

The behaviour after an interruption that happened mid-tool-call (state `Executing`) was read from `recover_interrupted_runs` and the idempotency code, not reproduced. The test list above includes that case so it is checked when the work is done.
