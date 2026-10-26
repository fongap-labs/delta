# Delta Foundation 合同

> 稳定、版本化的合同，定义 Delta Foundation 与 Delta Suite 之间的边界。合同是 Suite 与 Foundation 之间**唯一**的扩展边界。

本目录的机器读取规则与数据形状位于 [CONTRACT_RULES.md](CONTRACT_RULES.md)。该文件是唯一权威来源，本文不重复其中的规则，以免两处内容分叉。本目录中的中文说明属于 [CONTRACT_GOVERNANCE.md](../docs/governance/CONTRACT_GOVERNANCE.md) 第 7 节记录的语言例外。

## 合同类型

- **Capability ABI**：能力与 Worker 之间的边界。权威规范见 [capability-abi.md](../docs/architecture/capability-abi.md)。
- **Execution Grant（执行授权）**：把一次能力调用绑定到一次运行、一个执行纪元、一个权限范围与一个过期时间。它防止过期任务继续执行，并阻止跨范围或纪元之外的调用。其 JSON Schema 位于 `schemas/execution-grant.schema.json`。
- **Capability Request（能力请求）**：一次能力调用的请求结构，包括运行、工具调用、能力标识、截止时间、工作区、输入、权限与限制。
- **Analysis Result（分析结果）**：分析引擎写出的结果文件，带保证等级、引擎、环境、方法、输入与验证证据。其 JSON Schema 位于 `schemas/analysis-result.schema.json`。决定见 [ADR-0079](../docs/architecture/adr/0079-analysis-assurance-and-report-gate.md)。
- **Analysis Environment（分析环境）**：Explore / Validate Runtime Pack 的版本、平台、锁文件与包清单。其 JSON Schema 位于 `schemas/analysis-environment.schema.json`。决定见 [ADR-0080](../docs/architecture/adr/0080-analysis-runtime-packs.md)。
- **Method Pack（方法包）**：Suite 中带版本、环境与验证证据的方法集合。其 JSON Schema 位于 `schemas/method-pack.schema.json`。
- **Binding Manifest（绑定清单）**：报告渲染时写出的数字绑定记录，报告门禁据此核对结果文件与位置。其 JSON Schema 位于 `schemas/binding-manifest.schema.json`。
- **Product Edition（产品版本标记）**：已安装扩展写在自身目录下的显示标记，Foundation 的关于界面据此显示 `Delta` 或 `Delta Pro`；它不授予功能或权益。其 JSON Schema 位于 `schemas/product-edition.schema.json`。决定见 [ADR-0083](../docs/architecture/adr/0083-product-edition-marker.md)。

## 相关文档

- Suite 使用合同的途径与稳定性规则：[CONTRACT_RULES.md](CONTRACT_RULES.md)
- 合同的修改评审与兼容性要求：[CONTRACT_GOVERNANCE.md](../docs/governance/CONTRACT_GOVERNANCE.md)
- 扩展的激活方式：[extension-loading.md](../docs/architecture/extension-loading.md)
