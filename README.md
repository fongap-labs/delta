# Delta

Delta is a planned desktop-first distribution derived from the pinned `baseline/deepseek-harness` source of [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness). The `main` branch currently contains governance bootstrap code, **not a buildable Delta application or an imported Harness-based product tree**.

## Branch policy

| Branch | Purpose | Update policy |
| --- | --- | --- |
| `upstream/deepseek-harness` | Byte-preserving mirror of the official `master` history | Upstream synchronization only; never pushes to `main` |
| `baseline/deepseek-harness` | Pinned, pristine upstream source that Delta product development is based on | Advance only after compatibility review |
| `main` | Delta application source derived from the accepted baseline (not imported yet) | Governed PRs and Central CI |
| `legacy/v0.0.1` | Preserved historical source | Do not update |

The current baseline was initially created from the upstream mirror at official commit [`2e39e70496aa02986ca6a820c5d497e816b69d7e`](https://github.com/deepseek-ai/deepseek-harness/commit/2e39e70496aa02986ca6a820c5d497e816b69d7e). This is a **candidate**: build, desktop, plugin and security compatibility tests have not yet been completed. The commit is temporarily pinned by this branch and documented here; the machine-readable `upstream.lock.json` will be introduced with the first governed product/CI migration. **Do not release a Delta product from this metadata-only bootstrap.**

## Daily upstream comparison — always relative to the baseline

The authoritative daily gap report is **`baseline/deepseek-harness` → `upstream/deepseek-harness`**. Never use `main` as the base for upstream-change detection: Delta's product changes would make that report noisy and misleading. Resolve both branch tips to immutable commit SHAs and verify that the baseline is an ancestor of the upstream mirror; fail closed if the official source rewrites history.

- Report the baseline SHA, mirror SHA, number of upstream commits not yet adopted, changed files, breaking/compatibility-relevant changes, and a comparison link.
- An empty comparison means no upstream changes **since Delta's accepted baseline**; it does not prove Delta product compatibility.
- Run comparison as a read-only central Action Worker scheduled operation when implemented. It must not update the baseline, `main`, the lock, tags, or releases.
- A separate Delta-patch report may later compare `baseline/deepseek-harness` → `main` **after the first product import**. Do not compare these Git histories today: the bootstrap `main` and upstream do not have a common ancestor.
- The mirror branch exists, but automatic synchronization and the daily comparison scheduler are **not yet enabled**. The scheduling work is tracked in [Action Worker issue #1897](https://github.com/fongap-labs/action-worker/issues/1897).

For a local read-only check of the upstream gap after fetching both refs:

```sh
git fetch origin baseline/deepseek-harness upstream/deepseek-harness
git merge-base --is-ancestor origin/baseline/deepseek-harness origin/upstream/deepseek-harness
git log --oneline origin/baseline/deepseek-harness..origin/upstream/deepseek-harness
git diff --stat origin/baseline/deepseek-harness origin/upstream/deepseek-harness
```

If the ancestry check fails, stop and investigate rather than calculating or promoting a potentially unsafe delta.

## Baseline import and upgrade process

1. First initialize Delta product development **from the exact pristine source tree recorded in `baseline/deepseek-harness`**, with Delta-only adjustments isolated wherever possible. Preserve upstream licensing and plugin API contracts. This has **not** happened yet.
2. The first import must replace the metadata-only CI with real build, desktop and plugin compatibility tests **in the same governed PR**. Avoid mistaking a squash-only source-copy PR for Git ancestry: the existing bootstrap `main` has unrelated history. A one-time ancestry-preserving migration requires its own governance-approved strategy; do not force-push or bypass protected `main`.
3. For each later upstream update, always compute the change set between the **current accepted baseline SHA** and the desired new commit in `upstream/deepseek-harness`. Never compute upstream changes relative to Delta `main`.
4. Integrate those upstream changes into a temporary Delta integration branch, resolve Delta-specific changes, then run compatibility, plugin lifecycle, desktop smoke, security and multi-platform tests. Obtain PR governance and acceptance before promotion.
5. Only after Delta integration succeeds, advance the pristine `baseline/deepseek-harness` to the accepted **official** commit and update `upstream.lock.json` and a baseline tag in a controlled, auditable promotion. The lock and baseline must agree.
6. Release Delta only from centrally verified `main` source with Main Write Guard provenance.

**Automatic upstream synchronization, daily diff reporting, product import and baseline compatibility verification are not yet enabled.** A pinned reference is not equivalent to a validated release.

## Architecture guidelines

- Retain upstream Cordis and `dsh` plugin/API contracts; do not mass-rename upstream packages to `delta-*`.
- Keep branding, desktop experience and application-specific adapter code isolated from upstream packages wherever feasible.
- Use the existing Electron desktop carrier initially. Changing the carrier requires independently tested plugin and runtime compatibility.
- Keep central governance, CI orchestration and eventual release authorization in [`fongap-labs/action-worker`](https://github.com/fongap-labs/action-worker), not duplicate heavyweight GitHub workflows in Delta.
- The current [central CI execution manifest](.github/execution-manifest.json) is a bootstrap-only gate. Before introducing product source, replace this gate with real builds and tests in the **same PR**.

## Upstream acknowledgment

This project derives from [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness), licensed under MIT. Preserve upstream license and third-party notices when introducing upstream product files.
