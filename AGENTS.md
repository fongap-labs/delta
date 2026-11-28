# AGENTS

## Common rules

- Inspect the requested branch, source SHA, relevant code and existing contracts before editing. Do not read unrelated trees or expand the task.
- Reuse existing implementations; fix the root cause. Delete obsolete code. No speculative features, duplicate abstractions, unnecessary dependencies, fallback stacking or new workflows.
- Own boolean variables and parameters must use is/has/can/should: `enabled` -> `isEnabled`, `retry` -> `shouldRetry`; Python/Rust: `is_enabled`. Preserve external protocol names.
- Use precise language-native identifiers. Governed file names: `helper.ts` -> `validate-token-format.ts`. Own external configuration: `AW_TOKEN` -> `AW_STATUS_TOKEN`. Use three semantic segments; preserve standard platform names.
- Comments: add or retain only if omission would cause a concrete foreseeable error. Delete optional explanations, code narration, decorative separators and commented-out code. Preserve required legal notices, machine directives and external contracts.
- Tests: verify necessary observable behavior, security and regressions. Extend existing coverage before adding tests. Delete redundant or retired tests; never weaken useful coverage. Externalizable tests belong in the central test packs; local tests require an existing approved exception.
- Update existing authoritative documentation; no duplicate guides, task summaries or speculative plans.
- Commit and PR titles: only feat/fix/chore, in English (e.g. `fix(auth): reject invalid token`). Every commit requires a blank line and a substantive Chinese body explaining why and what changed. No AI attribution.
- Update only the Unreleased section of CHANGELOG.md with matching Chinese entries (feat=addition, fix=correction, chore=maintenance); do not rewrite released history.
- Verify live branch existence before edits. When main and enabled dev coexist, all changes start from dev and target dev by PR; main accepts only maintainer-authorized dev-to-main PR transfers. Never create a parallel main fix. With main only, target main by PR. If branch facts are uncertain or dev is unavailable/disabled despite existing, stop; never fall back. No direct protected-branch writes or unauthorized transfers or history rewrites.
- No secrets in code, logs or untrusted execution. No bypass of security, CI, merge or release gates. Never change tests merely to obtain green status.
- Run the applicable central preflight before submission; fix introduced failures. Verify the final diff and actual outcomes. Report unverified checks and blockers; never claim success without fresh evidence.

## Branch rules

- main is stable/production; dev is Beta only. When dev exists, use dev task branches and PRs only; promote to main solely by authorized dev-to-main PR transfer.

## Repository rules

- Keep Delta's product identity, local-first data and portable App/Data/Other layout. Do not revive upstream telemetry, automatic upstream updates or unapproved network transfers; never silently reuse Harness data or credentials.
- Preserve public plugin and tool permission contracts, necessary native identifiers, storage compatibility and third-party licenses. Do not introduce parallel Agent runtimes or speculative service boundaries.
- Keep capability ownership clear; use public module interfaces. Preserve approved functionality and reject unapproved upstream experimental implementations.
- First-party UI supports Simplified Chinese and English; keep application language independent from conversation language. Machine protocols and Agent instructions remain English.
- Read README.md, docs/delta/README.md and the applicable Delta architecture, privacy and acceptance contracts. Do not claim real Windows, plugin, browser or release acceptance from source CI alone.
- Follow directory-specific AGENTS.md for actual local contracts; they cannot override central rules or grant release and test exceptions.
