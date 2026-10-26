# ADR-0079：分析保证等级与报告门禁

状态：**已接受**（所有者，2026-10-27，同意按“探索 / 可验证 / 外部见证”三档组织分析能力）。

## 背景

- 蓝图把研究分析列为核心域，但当前 Foundation 的统计由 R 4.6.0 计算，Suite 首批又依赖 Foundation 的计算。语言、产品边界与报告权威混在一起。
- 讨论中出现的两种做法——“Foundation 全面 Python 化”和“R 全部进 Suite”——都把语言当成产品边界。语言不是客户购买的东西；客户购买的是“这个数字能否进正式报告”。
- 如果 Foundation 与 Suite 都能产出统计数字，而没有统一的保证等级，报告无法判断数字来源，两个引擎的微小差异会被带进正式成果。
- 现有结果文件 `delta.result` v1 只有 `values` 与 `provenance`，没有“保证等级”，无法在报告层做门禁。

## 决定

### 1. 三档保证等级

| 等级 | 用途 | 默认运行时 | 报告 |
|---|---|---|---|
| exploratory | 探索、清洗、描述、图表、快速检验、脚本分析 | Python（Explore Pack） | 只能进草稿，显著标注 |
| validated | DOE、稳定性/货架期、f2、ANCOVA、SPC、正式报告 | R（Validate Pack） | 可进正式报告，附证据 |
| attested | 用户已有的 SAS/JMP/Minitab/R 环境 | 外部适配器（后期） | 可进正式报告，附外部证据 |

等级是结果合同的一部分，不靠产品名或运行时推断。

### 2. 产品边界

- Foundation 提供 Explore；它是完整的一般分析工作台，但不是监管分析产品。
- Suite 提供 Validate（以及后期的 Attest）；监管级方法、验证证据和正式报告模板属于 Suite。
- Foundation 不要求安装 Suite；Suite 只通过公开合同接入，不导入 Foundation 内部。
- 语言不再定义产品：Explore 默认 Python，Validate 默认 R；方法包可以按方法选择引擎，但报告门禁只看保证等级与证据。

### 3. 结果合同 v2

新增 `delta.result` v2，至少包含：

- `assurance`：`exploratory`、`validated` 或 `attested`；
- `engine`：引擎名称、版本、运行时包与环境摘要；
- `method`：方法标识与方法版本；
- `inputs`：输入文件路径与 SHA-256；
- `parameters`、`values`、`warnings`、`limitations`；
- `validation`：仅 `validated` / `attested` 需要，指向验证报告与证据标识。

v1 文件继续可读，按 legacy 处理；新工具只写 v2。

### 4. 报告门禁

- `report_render` / `report_check` 继续使用现有的绑定清单。
- `report_check` 的 formal 模式要求每个被绑定的结果文件为 `validated` 或 `attested`。
- `exploratory` 结果在 formal 模式中失败，并给出原因；draft 模式允许，但报告必须显著标注。
- 没有 `assurance` 字段的 v1 结果按 legacy 接受，仅用于迁移；新工具不得再写 v1。
- 正式报告必须记录验证证据标识与环境摘要。

### 5. 方法归属

| 方法族 | Explore（Foundation） | Validate（Suite） |
|---|---|---|
| 数据导入、清洗、描述、EDA、图表 | Python curated 工具与受控脚本 | — |
| 基础 t 检验、ANOVA、相关、回归 | Python 探索版 | R 验证版 |
| DOE 设计生成 | 设计草稿与规划 | 可辩护的设计、分析与报告 |
| DOE 响应面分析、稳定性/货架期、f2、ANCOVA、SPC | — | R 方法包 |
| 机器学习、预测 | Python | 可通过后续方法包提供 |
| 正式报告 | 草稿 | 正式报告与证据包 |

不重复实现可报告方法；重叠方法只保留一个权威实现，Explore 版本必须明确标注为探索。

### 6. 迁移

- 分阶段：先定义合同与门禁，再实现 Explore / Validate Pack，最后移除 Foundation 的 R 统计。
- 不一次性切换；旧报告与旧结果文件保持可读。
- 具体运行时、打包与安全边界见 ADR-0080。

## 后果

- 新增 `schemas/analysis-result.schema.json`、`analysis-environment.schema.json`、`method-pack.schema.json` 与 `binding-manifest.schema.json`。
- `report_check` 的 formal 模式增加保证等级检查。
- 现有 R 统计结果继续作为 legacy；它们不因本 ADR 自动变成 validated。
- 蓝图 §2.2、§6.3、§8.2，以及 ADR-0054、ADR-0068、ADR-0070、ADR-0071、ADR-0073、ADR-0074、ADR-0078 中与本 ADR 冲突的部分，需要后续修订；在本 ADR 覆盖范围内以本 ADR 为准。

## 未验证事项与未决事项

- Python Runtime Pack 的分发方式见 ADR-0080，尚未实现。
- Suite 方法包的验证证据格式与首批方法尚未确定。
- 现有 v1 结果何时停止写入，需要迁移工具与弃用公告。
- `attested` 的外部工具适配器不在本次范围。

## 实现记录（2026-10-27）

- Foundation 的 R 统计已按 ADR-0081 立即移除；本 ADR 的 Explore / Validate 分层与报告门禁不变。
- Explore Pack 与 Suite Validate Pack 尚未实现。
