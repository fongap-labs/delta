# Delta

Delta is a planned desktop-first distribution based on [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness). The `main` branch currently contains governance bootstrap code, **not a buildable Delta application**.

## Branch policy

| Branch | Purpose | Update policy |
| --- | --- | --- |
| `upstream/deepseek-harness` | Byte-preserving mirror of the official `master` history | Upstream synchronization only; never pushes to `main` |
| `baseline/deepseek-harness` | Pinned, pristine DeepSeek Harness source for Delta | Advance only after compatibility review |
| `main` | Delta application source and eventual release branch | Governed PRs and Central CI |
| `legacy/v0.0.1` | Preserved historical source | Do not update |

The current baseline was initially created from the upstream mirror. It is only a **candidate**: build, desktop, plugin, and security compatibility tests have not yet been completed. [`upstream.lock.json`](upstream.lock.json) records the candidate's immutable commit SHA and `verification_status: pending`. **Do not release a Delta product from this metadata-only bootstrap.**

## Baseline upgrade process

1. Discover and synchronize official upstream changes into `upstream/deepseek-harness` (centralized in [Action Worker](https://github.com/fongap-labs/action-worker) when implementation is ready). Never automatically modify the baseline or product main branch.
2. Compare the candidate with the SHA in `upstream.lock.json`; run compatibility, plugin lifecycle, desktop smoke, security, and multi-platform build tests before proposing a promotion.
3. Create a temporary integration branch from `main`, apply the upstream changes and resolve Delta-specific patches. Do **not** merge the upstream history directly into the current bootstrap `main`: they have no common Git ancestor.
4. Following a successful governed upgrade, move the pristine baseline branch to the accepted official commit, record the immutable accepted SHA in the lock file, and keep a corresponding baseline tag.
5. Release Delta only from centrally verified `main` source with Main Write Guard provenance.

**Automatic upstream synchronization and baseline compatibility verification are not yet enabled**; keeping separate branches does not itself provide a scheduler or test coverage.

## Architecture guidelines

- Retain upstream Cordis and `dsh` plugin/API contracts; do not mass-rename upstream packages to `delta-*`.
- Keep branding, desktop experience and application-specific adapter code isolated from upstream packages wherever feasible.
- Use the existing Electron desktop carrier initially. Changing the carrier requires independently tested plugin and runtime compatibility.
- Keep central governance, CI orchestration and eventual release authorization in [`fongap-labs/action-worker`](https://github.com/fongap-labs/action-worker), not duplicate heavyweight GitHub workflows in Delta.
- The current [central CI execution manifest](.github/execution-manifest.json) is a bootstrap-only gate. Before introducing product source, replace this gate with real builds and tests in the **same PR**.

## Upstream acknowledgment

This project derives from [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness), licensed under MIT. Preserve upstream license and third-party notices when introducing upstream product files.
