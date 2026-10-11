# ADR-0055: Every figure in a report traces to a computed result

Status: **Accepted** (owner, 2026-10-11). Not implemented yet. Depends on [ADR-0054](0054-statistics-engine.md).

## Context

The blueprint requires that statistical outputs be traceable (`DELTA_BLUEPRINT.md` section 4.4: "统计前提和输出可追踪"). For research analysis, and for the formulation / CMC domain in particular, the most damaging failure is a plausible number in a report that no computation produced: a shelf life, a confidence limit, an f2 value, a model coefficient.

What exists (checked, not assumed):

1. **Validation rules.** `crates/delta-core/src/validation.rs` evaluates criteria against a tool's artifacts:
   - artifact count;
   - completeness;
   - required paths;
   - size gates;
   - required substrings;
   - CSV headers;
   - a valid-citation count.

   `required_substrings` reads the raw file bytes, so it is meaningful for text formats (Markdown, CSV) and not for DOCX, which is a ZIP container.
2. **Where criteria come from.** `RuntimeHost::validate_tool_artifacts` (`runtime/tool_execution.rs`) validates with the criteria a tool result supplies (`validation_criteria`), or a default (`min_artifacts: 0`, `max_artifacts: 50`, `require_complete: true`). Capability host results currently pass `validation_criteria: None` (`capability/host.rs`).
3. **Artifact registration.** Artifacts are registered per run with path, kind, size and SHA-256 (`artifact.rs`, `ArtifactRegistryWriter`). `ArtifactReader::list_for_run` lists one run's artifacts.
4. **Nothing links a number** in a report to the computation that produced it.

Extracting every number from free text and guessing where it came from is not deterministic: rounding, units, percentages, dates and section numbers all look like figures. The check has to be built on an explicit binding, not on inference.

## Options

**A. Free-text matching.** Extract all numbers from the report and require each to appear in some result artifact of the run.
- Plus: no change to how reports are written.
- Minus: heuristic; false positives (dates, section numbers) and false negatives (rounded or reformatted values); the verdict cannot be explained precisely.

**B. Bound figures (recommended).** Computational capabilities write a structured **result artifact**. Reports refer to its values through **bindings** that a renderer resolves. A new validation rule checks the bindings.
- Plus: deterministic; each figure has an exact source (result artifact, SHA-256, key); rounding is explicit; the trust strip can show it.
- Minus: reports must be produced through a renderer that understands bindings; narrative written outside it is not covered unless rule 3 below forbids unbound figures.

**C. Model self-declaration.** The model lists the figures it used and where they came from.
- Minus: the model reporting on itself is not a deterministic fact (blueprint section 4.4). Rejected.

## Decision: B

1. **Result artifact.** A computational capability (for example `statistics.*` from ADR-0054) writes a JSON result artifact. It contains:
   - `values`: named values, each with a `value`, a `unit` (optional) and a `kind` (`estimate`, `ci_lower`, `p_value`, ...);
   - `provenance`: engine and package versions, the script SHA-256, the input SHA-256s, the method and its parameters.

   It is registered like any artifact, so its SHA-256 is in the ledger.
2. **Bindings.** A report template or draft refers to a value as `{{result:<artifact-path>#<key>|<format>}}`, for example `{{result:out/shelf_life.json#shelf_life_months|%.0f}}`. A renderer capability (Markdown first, DOCX through `document.docx.render`) resolves each binding and writes the report. It also writes a **binding manifest**, a JSON artifact registered with the report. For every binding it records:
   - the result artifact path and its SHA-256;
   - the key, the raw value and the format;
   - the rendered text and its location (template section).
3. **New validation rule `numeric_trace`.** Criteria name a report and its binding manifest. The rule passes only if all of these hold:
   - every binding in the manifest points to a result artifact registered in the same run whose SHA-256 matches the ledger;
   - the key exists in that artifact;
   - formatting the raw value with the declared format gives exactly the rendered text;
   - the rendered text appears in the report at the recorded location. For DOCX, the rule reads the document's text from `word/document.xml`, and does not compare raw bytes.
   - `forbid_unbound_numbers`: the report's text may not contain digits outside bindings, except for an allowlist of patterns (section numbers, dates, conditions such as "25 °C/60% RH") declared in the criteria.
     - **On by default for every formal report.** A report is formal when the Skill's validation declares it `formal: true`; a Skill cannot switch the rule off for a formal report, only add allowlist patterns, and every allowlist pattern is shown with the verdict.
     - Drafts and exploratory notes that are not declared formal are not checked. The trust strip says "not a formal report: figures not checked" instead of showing a pass.
4. **Criteria supplied by Skills.** A Skill that produces a formal report declares `numeric_trace` criteria in its validation. The rule itself is generic and knows nothing about any domain. Domain rule sets (for example "the stability report must bind the shelf life and both confidence limits") live in Suite Skills.
5. **Surface.** The artifact trust strip already shows the validation verdict and the sources read (#132, #139). For a report, it adds "N figures bound to M results". The figure-level detail (value, result file, key, SHA-256) is shown on demand.

## Consequences

- `validation.rs` gains the `numeric_trace` rule with DOCX text extraction. This is Rust code in the Work authority; the parser has to be bounded (size and entry limits) like other untrusted file reads.
- The capability host must be able to pass `validation_criteria` from a Skill or capability result. Today it always passes `None`; that is a contract change to record in `runtime-public-contract.md`.
- Tests required:
  - a binding to a result artifact of another run fails;
  - an altered result artifact (SHA-256 mismatch) fails;
  - a missing key fails;
  - a rendered text that differs from the formatted value fails;
  - an unbound digit in a forbidden section fails, and an allowlisted pattern passes;
  - DOCX and Markdown reports are both checked;
  - oversized or malformed DOCX fails closed.
- No migration: existing artifacts and validations are unaffected.

## Owner's answers (2026-10-11)

1. B accepted.
2. Forbidding unbound figures is the default rule for formal reports.

## Not verified

- The DOCX text-location rule assumes reports are rendered from Delta templates. Arbitrary user-edited DOCX files after rendering are out of scope for this check; a later edit changes the SHA-256, and the validation then no longer applies to the edited file.
