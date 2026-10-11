# ADR-0056: Where a multi-round study keeps its design and decisions

Status: **Proposed** (2026-10-11). Awaiting the owner's decision.

## Context

Sequential experimental design is in scope from the first version (`DELTA_BLUEPRINT.md` section 2.2). A formulation or process study runs over weeks:

```text
design → round 1 → interim analysis → next round → ... → stop → confirmation
```

The work happens in many sessions. Each round has to answer:

- what is known;
- what is uncertain;
- what to run next;
- whether to stop.

The blueprint also requires that the system never changes the research objective, the allowed adjustment range or the stopping rule without explicit human control. Stability studies have the same shape: the protocol stays fixed while new time points arrive over months.

What exists (checked, not assumed):

1. The work fact model is Workspace → Session → Runs, plus Sources / Citations and Artifacts / Versions / Validation. The blueprint says not to add a WorkItem abstraction until real use proves it is needed (section 9).
2. Artifacts are registered per run with path and SHA-256 (`artifact.rs`). `ArtifactReader::list_for_run` lists one run's artifacts. There is no query for "all registrations of this path across runs".
3. Desktop projects group sessions in the sidebar, but they are stored on the device by the UI, not by the Runtime (CHANGELOG, sidebar projects). They are not a work fact.
4. Policy risk levels are in `policy.rs`; L4 ("irreversible or sensitive") is never auto-allowed. Skills declare a side-effect class (`skills.rs`).

## Options

**A. The study is an artifact (recommended for now).** A `study.json` file in the workspace, with a published schema, is the study record. Every change is a new registration of that artifact by a run, so the ledger holds its history. Changes to locked fields go through a dedicated capability that requires approval.
- Plus: no new Work object or store; consistent with the blueprint's "no new abstraction without evidence"; works across sessions because the file lives in the workspace.
- Minus: history across runs needs a new read query; the schema is a contract to version.

**B. A new Work object `Study` with its own ledger events.**
- Plus: first-class lifecycle and queries.
- Minus: a new top-level abstraction before real use has proven it; more Runtime surface.

**C. Reuse desktop projects.**
- Minus: UI state is not a work fact (invariant: the UI derives nothing). Rejected.

## Decision (proposed): A, revisit B with evidence

1. **Record.** The study record is `study.json`, validated against `schemas/study.schema.json`. It holds:
   - the objective;
   - the responses, with their targets or acceptance criteria;
   - the factors with their current ranges;
   - the allowed adjustments (outer bounds, factors that may be added);
   - the stopping rule;
   - the analysis plan (model, significance level, confidence level);
   - the rounds (design, data artifacts, analysis result artifacts);
   - the decisions (who, when, what, why).

   Domain fields (for example a stability protocol's storage conditions and pull points) go under an `extensions` object owned by the Suite Skill that uses them.
2. **Locked fields.** These fields are locked once the first round starts:
   - `objective`;
   - `responses`, with their targets or acceptance criteria;
   - `allowed_adjustments`: the outer bounds every factor range must stay within, and which new factors may be added;
   - `stopping_rule`;
   - `analysis_plan`: the planned model, significance level and confidence level.

   The current factor ranges are **not** locked. Narrowing, widening or adding a factor inside `allowed_adjustments` is the normal work of a sequential design (`DELTA_BLUEPRINT.md` section 2.2). Each such change is a decision entry with a reason, and starts only from the user's decision (point 3). A change that would leave `allowed_adjustments` is an amendment.

   Why this set: each locked field is one that, if changed after seeing the data, lets the result be steered toward a wanted answer. That covers the goal, what counts as success, how far the design may move, when to stop, and how the data will be judged. Factor ranges inside the declared bounds do not have that effect, so locking them would only force an approval on every round.

   Rules for locked fields:
   - They can only be changed through the `study.amend` capability.
   - `study.amend` is declared so that policy classifies it as L4, which is never auto-allowed. The approval card shows the old value, the new value and the reason.
   - The `study_schema` validation rule rejects any other run's registration of `study.json` that changes a locked field.
3. **Recommendations are not decisions.** The agent may append a recommendation for the next round. A round starts only when a decision entry, made by the user through an approval or a question, records it.
4. **History.** Add a read-only query that lists all registrations of a workspace path across runs (path, run id, SHA-256, time). The desktop shows a study's history from it.
5. **Revisit.** If real use shows that studies need their own lifecycle, sharing or queries that artifacts cannot give, propose B in a new ADR with that evidence.

## Consequences

- New schema `schemas/study.schema.json` (public contract, versioned).
- New validation rule `study_schema`: schema check plus the locked-field comparison with the previous registration.
- New capability `study.amend` (local write, approval required).
- New read query for path history in the artifact reader.
- Tests required:
  - a locked-field change outside `study.amend` fails validation;
  - `study.amend` without approval does not write;
  - an approved amendment records the old value, the new value and the reason;
  - history lists registrations across sessions in order;
  - a recommendation alone does not start a round.

## Owner's answers (2026-10-11)

On the locked set the owner asked for a recommendation. Decision point 2 now holds it:

- lock the objective, the responses with their targets, the outer adjustment bounds, the stopping rule and the analysis plan;
- keep factor ranges inside those bounds movable, each change recorded as a decision.

## Questions for the owner

1. Accept A, and revisit B only with evidence?
2. Confirm the locked set in decision point 2.
