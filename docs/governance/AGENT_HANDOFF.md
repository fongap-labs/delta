# Delta 开发交接入口

本文件不维护固定日期的 PR 状态、人员任务板或可复制的 Agent 系统提示词。过去的阶段性交接保留在 Git 历史中，不能作为当前任务事实。

## 事实来源

- 开发约束：[CLAUDE.md](../../CLAUDE.md) 与 [治理索引](README.md)。
- 产品边界：[Delta 蓝图](../DELTA_BLUEPRINT.md)、[Runtime 公共契约](../architecture/runtime-public-contract.md)、[Capability ABI](../architecture/capability-abi.md)。
- 技术决策：[ADR 索引](../architecture/adr/README.md)，涉及持久化时必须阅读 [ADR-0092](../architecture/adr/0092-runtime-result-durability.md)。
- 实际进度：[Delta PR](https://github.com/fongap-labs/delta/pulls)、[Delta Suite PR](https://github.com/fongap-labs/delta-suite/pulls) 及 [Action Worker CI](https://github.com/fongap-labs/action-worker/actions)。

## 交接最低内容

明确目标与范围、基线 SHA、已完成的变更、运行的验证和结果、未验证事项、兼容与回滚风险、下一步与阻塞条件。任何跨仓库任务均先重查当前代码与 PR，不沿用历史交接文档的旧状态。
