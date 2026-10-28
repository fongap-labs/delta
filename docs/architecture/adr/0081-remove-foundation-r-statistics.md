# ADR-0081：Foundation 立即移除 R 统计

状态：**已接受**（所有者，2026-10-27，要求立即移除 Foundation 的 R 统计）。

## 背景

- ADR-0079 把分析分为 `exploratory`、`validated`、`attested` 三档，并规定 Foundation 提供 Explore、Suite 提供 Validate。
- ADR-0080 规定 Explore 使用受控 Python Runtime Pack，Validate 使用 R 4.6.0 与版本化方法包。
- 在此之前，Foundation 仍注册着 12 个由 R 计算的工具（描述统计、检验、回归、ANCOVA、货架期、DOE、f2、结果记录与图表），并在 CI 中构建 R 4.6.0。
- 所有者要求不再等待双轨迁移，立即从 Foundation 移除 R 统计。Suite 的 CMC 领域代码目前读取 Foundation 的 `statistics_shelf_life` / `statistics_f2` 输出，因此 Suite 必须改为自己计算，属于紧随其后的迁移。

## 决定

### 1. 移除 Foundation 的 R 统计能力

删除 `projects/delta/crates/delta-core/src/statistics` 与以下工具注册：

- `statistics_describe`、`statistics_ttest`、`statistics_anova`、`statistics_chisq`、`statistics_correlation`、`statistics_regression`；
- `statistics_ancova`、`statistics_shelf_life`、`statistics_doe`、`statistics_f2`、`statistics_record`；
- `chart_render`。

同时删除只服务于这些工具的内置 `data-analysis` 技能与系统提示中的统计指令。

### 2. 移除 Foundation 的 R 工具链

- CI 不再安装或构建 R。
- Foundation 不再保留 R 的 external-vault 下载脚本、SHA-256 清单与操作文档；这些资产移到 Suite，用于将来维护 R 方法包。
- 旧的结果文件与报告保持可读；`report_render`、`report_check`、`numeric_trace`、`study` 继续存在。

### 3. Foundation 的当前状态

- Foundation 没有本地统计引擎，直到 Python Explore Pack 实现（ADR-0080）。
- 统计页面保留，但在没有本地工具时明确说明“本构建没有本地统计引擎”。
- 任何可报告数字必须来自 Suite 的 `validated` 结果；没有 Suite 时，模型不得自行计算数字。

### 4. Suite 的责任

- Suite 必须提供自己的 R 方法包与计算结果，不再读取 Foundation 的统计工具输出。
- 首批方法：稳定性货架期、溶出 f2、poolability/ANCOVA；随后是 DOE。
- Suite 输出必须使用 ADR-0079 的 `delta.result` v2，并带 `assurance: validated` 与验证证据。

## 取代关系

本 ADR 取代下列 ADR 中与 Foundation R 统计有关的部分；这些 ADR 的其他决定（数值溯源、研究记录、报告渲染等）继续有效：

- ADR-0054（Foundation 统计引擎由 R 计算）；
- ADR-0068（`data-analysis` 技能）；
- ADR-0070（R 基础绘图与 `chart_render`）；
- ADR-0071（Foundation 的 DOE 设计补充）；
- ADR-0073（Foundation 的 ANCOVA）；
- ADR-0074（Foundation 的货架期估计）；
- ADR-0078（Foundation 的 f2 与无模型统计入口）。

## 后果

- Foundation 的研究分析能力暂时收窄；这是有意的中间状态，不是最终产品边界。
- 旧报告仍可通过绑定清单核对；新报告在没有 Suite 时不能产生新的统计数字。
- 桌面统计页、系统提示、技能、CI 与文档需要同步修改。
- Suite 的 CMC 代码必须从“校验 Foundation 输出”改为“自己计算并输出 v2 结果”。

## 未验证事项与未决事项

- Python Explore Pack 尚未实现。
- Suite 的 R 方法包、验证证据与报告模板尚未实现。
- 现有 v1 结果文件的弃用时间与迁移工具尚未确定。
- Suite 仓库已克隆到 `D:\900 AIWork\910 GitHub\FongHub\delta-suite`，但本 ADR 的 Suite 迁移工作尚未开始。
