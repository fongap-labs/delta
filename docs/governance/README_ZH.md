# Delta 治理

Delta 继承 [Fongap Labs Action Worker](https://github.com/fongap-labs/action-worker/tree/main/docs) 的共用工程治理。本目录只保留 Delta 自身的产品、契约、依赖、质量、发布与扩展规则。

## 项目级权威

| 领域 | 文档 | 作用 |
|---|---|---|
| 产品 | [../DELTA_BLUEPRINT.md](../DELTA_BLUEPRINT.md) | 长期产品边界 |
| 架构 | [../architecture/target-architecture.md](../architecture/target-architecture.md) | 长期系统架构 |
| Runtime | [../architecture/runtime-public-contract.md](../architecture/runtime-public-contract.md) | 当前公开 Runtime 契约 |
| Capability | [../architecture/capability-abi.md](../architecture/capability-abi.md) | Capability / Worker 边界 |
| Foundation ↔ 扩展 | [REPOSITORY_BOUNDARY.md](REPOSITORY_BOUNDARY.md) | 仓库与权威边界 |
| 公开契约 | [CONTRACT_GOVERNANCE.md](CONTRACT_GOVERNANCE.md) | 契约演进 |
| 依赖 | [DEPENDENCY_POLICY.md](DEPENDENCY_POLICY.md) | Delta 特有依赖规则 |
| 质量 | [QUALITY_POLICY.md](QUALITY_POLICY.md) | Delta 特有验证要求 |
| 发布 | [RELEASE_POLICY.md](RELEASE_POLICY.md) | Delta 发布兼容关系与产物 |

## 专项基线

`baseline/` 中其余文件只负责 Delta 特有的许可、安全、隐私、扩展分发、贡献与兼容问题，不再定义第二套架构权威。

## 架构决策

- [0001-one-delta-one-core.md](../architecture/adr/0001-one-delta-one-core.md)
- [0002-foundation-suite-boundary.md](../architecture/adr/0002-foundation-suite-boundary.md) —— 文件名保留历史记录；当前决策是 Foundation ↔ 可选扩展
- [0003-capability-over-fork.md](../architecture/adr/0003-capability-over-fork.md)
- [0004-capability-execution-chain.md](../architecture/adr/0004-capability-execution-chain.md)

## 权威顺序

```text
Action Worker 共用治理
        ↓
DELTA_BLUEPRINT / target-architecture
        ↓
Runtime / Capability / Repository 契约
        ↓
Delta 专项 Policy
        ↓
实现文档
```

Delta Foundation 掌握产品权威；可选扩展只能通过稳定公开契约和隔离清单增加能力。
