# Delta 治理

本目录只保留 Delta Foundation 以及公开扩展生态需要知道的治理规则。

## 基线文档

| 领域 | 文档 | 作用 |
|---|---|---|
| 架构 | [ARCHITECTURE_GOVERNANCE.md](baseline/ARCHITECTURE_GOVERNANCE.md) | Core 架构与权威边界 |
| 源码与许可 | [SOURCE_LICENSING_GOVERNANCE.md](baseline/SOURCE_LICENSING_GOVERNANCE.md) | Foundation 源码与第三方合规 |
| 发布与兼容 | [RELEASE_VERSIONING_GOVERNANCE.md](baseline/RELEASE_VERSIONING_GOVERNANCE.md) | Foundation 与扩展兼容关系 |
| 扩展分发 | [EXTENSION_DISTRIBUTION_GOVERNANCE.md](baseline/EXTENSION_DISTRIBUTION_GOVERNANCE.md) | 扩展发现、隔离与生命周期 |
| 安全 | [SECURITY_GOVERNANCE.md](baseline/SECURITY_GOVERNANCE.md) | 安全基线 |
| 遥测与隐私 | [TELEMETRY_PRIVACY_GOVERNANCE.md](baseline/TELEMETRY_PRIVACY_GOVERNANCE.md) | 数据与隐私规则 |
| 贡献 | [CONTRIBUTION_GOVERNANCE.md](baseline/CONTRIBUTION_GOVERNANCE.md) | 贡献规则 |
| 弃用与迁移 | [DEPRECATION_MIGRATION_GOVERNANCE.md](baseline/DEPRECATION_MIGRATION_GOVERNANCE.md) | 兼容与迁移 |

## 当前仓库规则

- [REPOSITORY_BOUNDARY.md](REPOSITORY_BOUNDARY.md) —— Foundation 与可选扩展边界
- [CONTRACT_GOVERNANCE.md](CONTRACT_GOVERNANCE.md) —— 公开契约演进
- [DEVELOPMENT_POLICY.md](DEVELOPMENT_POLICY.md) —— 开发规则
- [DEPENDENCY_POLICY.md](DEPENDENCY_POLICY.md) —— 依赖规则
- [QUALITY_POLICY.md](QUALITY_POLICY.md) —— 质量门禁
- [RELEASE_POLICY.md](RELEASE_POLICY.md) —— 发布检查
- [AI_AGENT_RULES.md](AI_AGENT_RULES.md) —— AI 编码 Agent 规则

## 架构决策

- [0001-one-delta-one-core.md](../architecture/adr/0001-one-delta-one-core.md)
- [0002-foundation-suite-boundary.md](../architecture/adr/0002-foundation-suite-boundary.md) —— 文件名保留历史兼容，内容已收敛为 Foundation 与可选扩展边界
- [0003-capability-over-fork.md](../architecture/adr/0003-capability-over-fork.md)
- [0004-capability-execution-chain.md](../architecture/adr/0004-capability-execution-chain.md)

## 一句话边界

> Delta Foundation 掌握产品权威；可选扩展只能通过稳定公开契约和隔离清单增加能力。
