# Delta 治理

本目录是 Delta 治理文档的入口，说明各文档的职责、权威顺序与专项基线。Delta 继承 [Fongap Labs Action Worker](https://github.com/fongap-labs/action-worker/tree/main/docs) 的共用工程治理。本目录只保留 Delta 自身的产品、合同、依赖、质量、发布与扩展规则。

## 项目级权威

| 领域 | 文档 | 作用 |
|---|---|---|
| 产品 | [../DELTA_BLUEPRINT.md](../DELTA_BLUEPRINT.md) | 长期产品边界 |
| 架构 | [../architecture/target-architecture.md](../architecture/target-architecture.md) | 长期系统架构 |
| Runtime | [../architecture/runtime-public-contract.md](../architecture/runtime-public-contract.md) | 当前公开 Runtime 合同 |
| Capability | [../architecture/capability-abi.md](../architecture/capability-abi.md) | Capability / Worker 边界 |
| Foundation ↔ 扩展 | [REPOSITORY_BOUNDARY.md](REPOSITORY_BOUNDARY.md) | 仓库与权威边界 |
| 公开合同 | [CONTRACT_GOVERNANCE.md](CONTRACT_GOVERNANCE.md) | 合同演进 |
| 依赖 | [DEPENDENCY_POLICY.md](DEPENDENCY_POLICY.md) | Delta 特有依赖规则 |
| 质量 | [QUALITY_POLICY.md](QUALITY_POLICY.md) | Delta 特有验证要求 |
| 发布 | [RELEASE_POLICY.md](RELEASE_POLICY.md) | Delta 发布兼容关系与产物 |

## 专项基线

`baseline/` 中其余文件覆盖 Delta 特有的许可、安全、隐私、扩展分发、贡献与兼容问题，不再定义第二套架构权威。

`baseline/` 文件中引用的 Architecture Governance 章节（如 §36）、Dual-Repo Design 等文档未收录于本仓库，相关条款以其对应权威来源为准。

## 架构决策

- [0001-one-delta-one-core.md](../architecture/adr/0001-one-delta-one-core.md)
- [0002-foundation-suite-boundary.md](../architecture/adr/0002-foundation-suite-boundary.md)（文件名保留历史记录；当前决策是 Foundation ↔ 可选扩展）
- [0003-capability-over-fork.md](../architecture/adr/0003-capability-over-fork.md)
- [0004-capability-execution-chain.md](../architecture/adr/0004-capability-execution-chain.md)

完整索引见 [ADR 索引](../architecture/adr/README.md)。

## 权威顺序

```text
Action Worker 共用治理
        ↓
DELTA_BLUEPRINT / target-architecture
        ↓
Runtime / Capability / Repository 合同
        ↓
Delta 专项 Policy（策略）
        ↓
实现文档
```

Delta Foundation 掌握产品权威；可选扩展只能通过稳定公开合同和隔离清单增加能力。
