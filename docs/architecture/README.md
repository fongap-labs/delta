# 技术架构

这里收录 Delta 产品架构和当前运行时的技术资料。目标架构不应表述为已经实现的模块。

## 核心资料

- [Delta 应用架构与演进边界](../../governance/ARCHITECTURE.md)：产品目标及现行与目标架构的区别。
- [现有 Harness 技术架构](../architecture.zh.md)：运行时组成、扩展点及依赖关系。
- [Agent 生命周期](../agent-lifecycle.zh.md)
- [能力服务边界](../capability-seams.zh.md)
- [模块依赖关系](../module-graph.zh.md)
- [Session 格式状态](../session-format-status.zh.md)

## 维护原则

有关实际运行行为，优先以当前 Delta 代码与经过验证的技术文档为准；历史 Harness 文档是兼容性参考，不能覆盖 Delta 产品边界或中央工程规则。

详细类型定义、接口和生成式目录请从 [技术参考](../reference/README.md) 查阅。
