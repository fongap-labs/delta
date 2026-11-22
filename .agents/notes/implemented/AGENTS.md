# 已实施决策记录维护指令

本目录记录已经真实落地的决策。遵循[仓库指令](../../../AGENTS.md)、[中文文档规范](../../../docs/AGENTS.md)和[决策记录格式](../README.md#the-file-format)；现有 `verify-agent-note-format` 负责检查生命周期对应的文档结构。

## 保持已实现事实准确

实现更改时，必须同步修正对应记录中的路径、符号、默认值和机制，而不是追加流水式变更历史。已经不再帮助今后维护的记录，应按[归档流程](../../skills/dsh-archive-agent-notes/SKILL.md)移入冻结历史，连同其完整文件组合一起处理。

<a id="this-is-not-a-license-to-rewrite-the-decision"></a>

## 不得擅自改写原始决策

已实现的具体事实可以更新，但改变原始决策或理由需要新的 Agent Note 和交叉引用。只有满足决策记录的归并规则时，才可清理被完全取代的原记录；不能把事实更新当作改写历史的理由。
