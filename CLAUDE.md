# Delta Agent Guide

Read the organization rules first:

1. https://github.com/fongap-labs/action-worker/blob/main/docs/01_SHARED_GOVERNANCE.md
2. https://github.com/fongap-labs/action-worker/blob/main/docs/03_NAMING_CONVENTIONS.md
3. https://github.com/fongap-labs/action-worker/blob/main/docs/04_CHANGELOG_CONVENTIONS.md
4. https://github.com/fongap-labs/action-worker/blob/main/docs/05_INTEGRATION_GUIDE.md

Then read local `docs/GOVERNANCE.md`, `docs/ARCHITECTURE.md`, `docs/MIGRATION_PLAN.md`, `docs/BRAND.md`, and task-relevant code/contracts.

Architecture boundaries:
- Desktop owns user experience, not privileged execution authority.
- Core owns shared trust, session state, approval and plugin management.
- One selected Runtime owns each session's Agent loop: native Delta or external Harness.
- Harness-specific details belong exclusively in versioned Adapters.
- Workers execute scoped jobs; no direct write to core state or approval authority.
- Host mode must preserve host-native plugins.

Use PRs and central CI. Do not weaken governance, change trusted permissions or release without the maintainer's explicit scope-bound authorization. Do not claim a protocol or host takeover is functional until verified.

