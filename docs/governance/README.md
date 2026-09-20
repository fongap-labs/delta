# Delta Governance

Delta inherits shared engineering governance from [Fongap Labs Action Worker](https://github.com/fongap-labs/action-worker/tree/main/docs). This directory keeps only Delta-specific product, contract, dependency, quality, release and extension rules.

## Project authority

| Area | Document | Purpose |
|---|---|---|
| Product | [../DELTA_BLUEPRINT.md](../DELTA_BLUEPRINT.md) | Long-term product boundary |
| Architecture | [../architecture/target-architecture.md](../architecture/target-architecture.md) | Long-term system architecture |
| Runtime | [../architecture/runtime-public-contract.md](../architecture/runtime-public-contract.md) | Current public Runtime contract |
| Capability | [../architecture/capability-abi.md](../architecture/capability-abi.md) | Capability / Worker boundary |
| Foundation ↔ extensions | [REPOSITORY_BOUNDARY.md](REPOSITORY_BOUNDARY.md) | Repository and authority boundary |
| Public contracts | [CONTRACT_GOVERNANCE.md](CONTRACT_GOVERNANCE.md) | Contract evolution |
| Dependencies | [DEPENDENCY_POLICY.md](DEPENDENCY_POLICY.md) | Delta-specific dependency rules |
| Quality | [QUALITY_POLICY.md](QUALITY_POLICY.md) | Delta-specific validation requirements |
| Release | [RELEASE_POLICY.md](RELEASE_POLICY.md) | Delta release compatibility and artifacts |

## Specialized baselines

The remaining files under `baseline/` cover Delta-specific licensing, security, privacy, extension distribution, contribution and compatibility concerns. They do not define a second architecture authority.

## Architecture decisions

- [0001-one-delta-one-core.md](../architecture/adr/0001-one-delta-one-core.md)
- [0002-foundation-suite-boundary.md](../architecture/adr/0002-foundation-suite-boundary.md) — historical filename; current decision is Foundation ↔ optional extensions
- [0003-capability-over-fork.md](../architecture/adr/0003-capability-over-fork.md)
- [0004-capability-execution-chain.md](../architecture/adr/0004-capability-execution-chain.md)

## Authority order

```text
Action Worker shared governance
        ↓
DELTA_BLUEPRINT / target-architecture
        ↓
Runtime / Capability / Repository contracts
        ↓
specialized Delta policy
        ↓
implementation docs
```

Delta Foundation owns the product authority. Optional extensions add capabilities only through stable public contracts and isolated manifests.
