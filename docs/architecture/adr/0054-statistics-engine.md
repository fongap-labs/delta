# ADR-0054: Which statistics engine produces reportable figures

Status: **Accepted** (owner, 2026-10-11), **amended** the same day: R is invoked directly by a Rust-native capability, not through a Python worker (see Amendment). Not implemented yet.

## Context

The blueprint makes research analysis one of Delta's three product domains (`DELTA_BLUEPRINT.md` section 2.2): descriptive statistics, inference, regression, DOE, sequential design and a formal report. The first target domain is formulation / CMC research analysis, delivered as an extension in `fongap-labs/delta-suite`; Foundation keeps the generic statistics capability (owner, 2026-10-11).

Today Delta has no statistics capability. What exists (checked, not assumed):

1. The Capability ABI runs process workers through `WorkerProcessRunner` (`crates/delta-core/src/capability/runners.rs`), which accepts any program: Python, PowerShell and Shell have helpers, "and other controlled workers" use `WorkerProcessRunner::new(program, arguments)`.
2. The only worker SDK is the Python `delta-worker-sdk` (`packages/delta_worker_sdk`).
3. The blueprint lists Python, PowerShell and Shell as task execution languages (section 8.2). R is not listed.
4. The desktop app does not ship Python. `packaging/portable/build-portable.ps1` states that Python is not an application dependency, and ADR-0058 records that the Rust runtime does not call the Python tools under `integrations/`. Baseline capabilities (documents, web) are Rust-native capabilities registered in `CapabilityHost::product_defaults`.
5. Without a statistics capability the model would compute or recall numbers itself. For formulation / CMC users (stability shelf life, dissolution f2, DOE) a number that is not produced by a deterministic, versioned engine is not usable.

The owner asked that the statistics stack use libraries regulators accept (2026-10-11). That needs to be made precise, because no regulator publishes a list of approved statistical libraries:

- The FDA's *Statistical Software Clarifying Statement* says the FDA does not require any specific statistical software; the sponsor is responsible for documenting the software and showing it is reliable for its use. As far as we know, EMA and NMPA also do not certify statistical software.
- What regulators look at in practice is **documented validation and fitness for intended use**, plus precedent in submissions. By that measure:
  - **SAS** has the longest submission history (mostly clinical).
  - **R** has a published compliance basis: the R Foundation's *R: Regulatory Compliance and Validation Issues — A Guidance Document for the Use of R in Regulated Clinical Trial Environments*, the R Validation Hub's risk-based package assessment (`riskmetric`), and R Consortium pilot submissions to the FDA built in R.
  - **JMP and Minitab** are the tools CMC and formulation scientists most often use for DOE, stability and capability analyses.
  - **Python** (SciPy, statsmodels) is numerically sound but has no comparable regulatory validation guidance or submission precedent at the package level.

The facts about regulatory statements and pilots above are from public documents as known at the time of writing. Check them against the current official texts before quoting them in any user-facing or validation material.

## Options

**A. Python only (NumPy, SciPy, statsmodels) inside the existing Python worker SDK.**
- Plus: no new runtime; one SDK; simple packaging.
- Minus: does not meet the owner's requirement. There is no regulatory validation guidance or submission precedent to point to, so every user would have to justify the stack themselves.

**B. R as the engine for reportable figures, driven by a Python worker (recommended).**
- A Python worker built on `delta-worker-sdk` receives the job, prepares the inputs, runs a fixed R script with `Rscript`, and returns the results through the existing ABI.
- Base R (`stats`) and a small, pinned set of CRAN packages compute every figure that can reach a formal report.
- Plus: meets the requirement with a documented compliance basis; keeps one worker SDK and the existing ABI; R is used only as a computation engine, not as an authority.
- Minus: adds R as a task execution language; R must be present (packaging); package versions must be pinned and risk-assessed.

**B2. R as the engine, invoked directly by a Rust-native capability (chosen by the amendment).**
- A capability registered in `CapabilityHost::product_defaults` runs a fixed R entry script with `Rscript` through `WorkerProcessRunner`, which already runs any program under the Runtime's timeout and cancellation.
- Plus: the user needs R only, not R and Python; one fewer process between the Runtime and the computation; cancellation reaches the R process directly; the validation pack covers R alone.
- Minus: a small R-side interface for inputs and results; Rust prepares input tables (it already reads XLSX, CSV and DOCX since ADR-0058).

**C. Drive the user's own validated JMP, Minitab or SAS installation.**
- Plus: uses the tools users already validated.
- Minus: commercial licences; scripting and automation differ per tool and per platform; Delta cannot ship or test against them. This is better as a later export or adapter in Suite (for example, emitting a JMP script) than as the engine.

## Decision: B2 (amended from B), with optional JMP / SAS script export

1. **Engine.** Any figure that can appear in a formal Artifact (report, table, chart caption) is computed by R. Rust may only read, reshape and validate input data and render results; it does not compute reportable statistics.
2. **Shape.** The Foundation capability is `statistics.*` (for example `statistics.describe`, `statistics.regression`, `statistics.anova`, `statistics.ancova`), a Rust-native capability that runs `Rscript` with R scripts compiled into the binary (so their SHA-256 is fixed per build). The R process is a controlled worker under the Capability ABI: scoped inputs, staging output, timeout and cancel, no authority.
   - **Interface in base R only.** Rust writes the input table as CSV and the parameters as a DCF file (`read.dcf`, base R). The R script writes its results as DCF, with every number printed by `sprintf("%.17g")` so the double round-trips exactly. No JSON package is needed, so Foundation stays on base R and recommended packages (point 4).
3. **Pinned environment.**
   - The R version and every package version are pinned.
   - The R version is the current R release (the release CRAN publishes as current) at the time the worker is built, pinned to its exact `x.y.z`.
   - Moving to a newer R release is a deliberate update: the lock changes, the reference tests (point 6) pass on the new version, and the CHANGELOG names both versions.
   - A package lock is shipped with the worker, for example an `renv.lock` or a dated repository snapshot.
   - At start-up the worker checks that the installed environment matches the lock. If it does not, it fails closed with a typed error; it never runs on an unpinned environment.
4. **Package admission.**
   - Foundation uses base R and recommended packages.
   - Every other package needs a recorded risk assessment (R Validation Hub method) and a licence check against `DEPENDENCY_POLICY.md` before it is admitted.
   - Domain packages (for example a bootstrap f2 implementation) are admitted and pinned in Suite, not in Foundation.
5. **Provenance in every result.** The result payload carries:
   - the R version;
   - the package names and versions actually loaded;
   - the SHA-256 of the R script and of each input file;
   - the method name and its parameters.

   This is what [ADR-0055](0055-numeric-traceability.md) binds report figures to.
6. **Verification.** Each `statistics.*` function has reference tests that must pass before release:
   - NIST Statistical Reference Datasets (StRD) for numeric accuracy (linear regression, ANOVA);
   - worked examples from published guidance;
   - an independent cross-check against statsmodels/SciPy in the test suite.

   A mismatch beyond a stated tolerance fails the release.
7. **Availability.** The portable build does not bundle R in the first version. The capability detects a supported R installation; if none is found, it reports itself unavailable with a clear reason and never falls back to another engine. Bundling is a separate packaging decision (size, GPL licence obligations for redistributed binaries).
8. **Blueprint.** Section 8.2 adds R to the task execution languages, with the same Worker rules (scoped input and output, timeout, least privilege, no authority).
9. **Optional JMP / SAS script export (option C as an export, not as the engine).** A Suite adapter may write, next to a result, a JMP (JSL) or SAS script that reproduces the same analysis on the same input file.
   - It is optional and off unless the user asks for it.
   - It writes a script artifact; it never runs JMP or SAS, and never feeds their output back into a Delta report.
   - Reportable figures in Delta always come from the R engine. The export lets a user re-run the analysis in a tool their organisation has already validated and compare the results.

## Consequences

- A new `statistics` module in `delta-core` (registration, input preparation, result parsing) and its R scripts, registered in `CapabilityHost::product_defaults` beside the document and web capabilities.
- No change to the ABI or the Python worker SDK. `WorkerProcessRunner` runs `Rscript` directly, inside the job's timeout and cancellation.
- A validation pack grows with the capability:
  - the package risk assessments;
  - the reference test results per release;
  - the environment lock.

  This is the material a user's QA needs to judge fitness for intended use. Delta does not claim the capability is "validated" or "accepted by regulators".
- CI needs an R toolchain on the runners that execute the statistics tests (central CI in Action Worker).

## Owner's answers (2026-10-11)

1. B accepted: R computes, Python orchestrates and handles data and rendering.
2. Optional JMP / SAS script export is planned in Suite (decision point 9).
3. Pin the current R release, not the previous minor version (decision point 3).
4. Amendment: R is invoked directly by a Rust-native capability, not through a Python worker, because the desktop app does not ship Python (option B2).

## Amendment (2026-10-11)

The first decision (B) had a Python worker drive R. The desktop app does not ship Python (Context, point 4), so B would have required every user to install both Python and R for statistics, and would have put a Python process between the Runtime and R. The owner chose B2. Everything else in this ADR is unchanged: R computes every reportable figure, the environment is pinned and checked, every result carries its provenance, and reference tests gate releases. Python (statsmodels) remains in the test suite only, as the independent cross-check of point 6.

## Not verified

- The statements about FDA, EMA, NMPA and R Consortium pilots are from public documents as known at the time of writing, not re-read for this ADR.
- No R capability has been built yet. On Windows `Rscript.exe` starts the R engine as a further process; cancellation must end that process tree (for example through a job object, as the controlled browser does) and must be tested.
