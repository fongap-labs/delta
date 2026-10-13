# Delta Foundation 合同

> 稳定、版本化的合同，定义 Delta Foundation 与 Delta Suite 之间的边界。合同是 Suite 与 Foundation 之间**唯一**的扩展边界。

本目录的机器读取规则与数据形状位于 [CONTRACT_RULES.md](CONTRACT_RULES.md)。该文件是唯一权威来源，本文不重复其中的规则，以免两处内容分叉。本目录中的中文说明属于 [CONTRACT_GOVERNANCE.md](../docs/governance/CONTRACT_GOVERNANCE.md) 第 7 节记录的语言例外。

## 合同类型

- **Capability ABI**：能力与 Worker 之间的边界。权威规范见 [capability-abi.md](../docs/architecture/capability-abi.md)。
- **Execution Grant（执行授权）**：把一次能力调用绑定到一次运行、一个执行纪元、一个权限范围与一个过期时间。它防止过期任务继续执行，并阻止跨范围或纪元之外的调用。其 JSON Schema 位于 `schemas/execution-grant.schema.json`。
- **Capability Request（能力请求）**：一次能力调用的请求结构，包括运行、工具调用、能力标识、截止时间、工作区、输入、权限与限制。

## 相关文档

- Suite 使用合同的途径与稳定性规则：[CONTRACT_RULES.md](CONTRACT_RULES.md)
- 合同的修改评审与兼容性要求：[CONTRACT_GOVERNANCE.md](../docs/governance/CONTRACT_GOVERNANCE.md)
- 扩展的激活方式：[extension-loading.md](../docs/architecture/extension-loading.md)
