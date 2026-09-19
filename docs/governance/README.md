# Delta Governance

This directory contains public governance for Delta Foundation and its extension
ecosystem.

## Baselines

| Area | Document | Purpose |
|---|---|---|
| Architecture | [ARCHITECTURE_GOVERNANCE.md](baseline/ARCHITECTURE_GOVERNANCE.md) | Core architecture and authority rules |
| Source & licensing | [SOURCE_LICENSING_GOVERNANCE.md](baseline/SOURCE_LICENSING_GOVERNANCE.md) | Foundation source and third-party compliance |
| Release & compatibility | [RELEASE_VERSIONING_GOVERNANCE.md](baseline/RELEASE_VERSIONING_GOVERNANCE.md) | Foundation and extension compatibility |
| Extension distribution | [EXTENSION_DISTRIBUTION_GOVERNANCE.md](baseline/EXTENSION_DISTRIBUTION_GOVERNANCE.md) | Extension discovery, isolation and lifecycle |
| Security | [SECURITY_GOVERNANCE.md](baseline/SECURITY_GOVERNANCE.md) | Security baseline |
| Telemetry & privacy | [TELEMETRY_PRIVACY_GOVERNANCE.md](baseline/TELEMETRY_PRIVACY_GOVERNANCE.md) | Data and privacy rules |
| Contributions | [CONTRIBUTION_GOVERNANCE.md](baseline/CONTRIBUTION_GOVERNANCE.md) | Contribution rules |
| Deprecation | [DEPRECATION_MIGRATION_GOVERNANCE.md](baseline/DEPRECATION_MIGRATION_GOVERNANCE.md) | Compatibility and migration |

## Active repository rules

- [REPOSITORY_BOUNDARY.md](REPOSITORY_BOUNDARY.md) — Foundation ↔ optional extension boundary
- [CONTRACT_GOVERNANCE.md](CONTRACT_GOVERNANCE.md) — public contract evolution
- [DEVELOPMENT_POLICY.md](DEVELOPMENT_POLICY.md) — development rules
- [DEPENDENCY_POLICY.md](DEPENDENCY_POLICY.md) — dependency policy
- [QUALITY_POLICY.md](QUALITY_POLICY.md) — quality gates
- [RELEASE_POLICY.md](RELEASE_POLICY.md) — release checks
- [AI_AGENT_RULES.md](AI_AGENT_RULES.md) — AI coding-agent rules

## Architecture decisions

- [0001-one-delta-one-core.md](../architecture/adr/0001-one-delta-one-core.md)
- [0002-foundation-suite-boundary.md](../architecture/adr/0002-foundation-suite-boundary.md) — historical filename; current decision is Foundation ↔ optional extensions
- [0003-capability-over-fork.md](../architecture/adr/0003-capability-over-fork.md)
- [0004-capability-execution-chain.md](../architecture/adr/0004-capability-execution-chain.md)

## Public boundary in one sentence

> Delta Foundation owns the product authority; optional extensions add
> capabilities only through stable public contracts and isolated manifests.
