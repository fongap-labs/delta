---
name: data-analysis
description: 对工作区里的 CSV 做统计分析，并写出数字可追溯的 Markdown 报告（data analysis, statistics report）：t 检验、方差分析、卡方、相关、回归、析因试验设计。数字由 R 计算并绑定到结果文件。
source: builtin
version: 1
side_effect: local_write
capabilities: ["file.read", "statistics.ttest", "statistics.anova", "statistics.chisq", "statistics.correlation", "statistics.regression", "statistics.doe", "statistics.record", "chart.render", "report.render", "report.check"]
---

# 统计分析与可追溯报告

用于对工作区里的 CSV 做统计分析，并交付一份每个数字都能追溯到计算结果的报告。系统提示里已经规定了主流程（不自己算数、用 statistics_record、用 report_render 和 report_check）；本技能只补充选方法、报告写法和容易踩的坑。你做不了的事要直说：图表只能以独立的 PNG 文件交付，不能插入 Word 或 Excel；不检验统计前提；不能处理 CSV 以外的数据文件。

## 开始前先问清（一次问完，不要连环追问）

用 ask_user 一次问清这几件事，用户已经说过的不要再问：

1. 要回答什么问题：比较两组或多组？两个变量是否相关？一个结果由哪些因素解释？
2. 数据文件的路径，哪一列是结果，哪一列分组，有没有配对。
3. 要不要正式报告，置信水平是否另有要求（默认 0.95）。

## 选方法

| 问题 | 工具 |
|---|---|
| 一列的均值和某个值比 | statistics_ttest，kind 为 one_sample，用 mu 给出该值 |
| 两组比较 | statistics_ttest，kind 为 two_sample，group 是分组列；默认 Welch，方差相等才设 equal_variance |
| 同一对象前后比较 | statistics_ttest，kind 为 paired，column 和 column_b 是两列 |
| 三组及以上比较 | statistics_anova，response 是数值列，group 是分组列 |
| 两个分类变量是否有关 | statistics_chisq |
| 两个数值列是否相关 | statistics_correlation，method 为 pearson、spearman 或 kendall |
| 一个结果由多个数值列解释 | statistics_regression，predictors 为 1 到 8 列，需要时开 interactions |
| 设计两水平析因试验 | statistics_doe；做完试验、填好响应列后用 statistics_regression 分析 |
| 只想看一列的概况 | statistics_describe（只在对话里用，见下面"不能绑定的数字"） |
| 画分布、比较、相关或趋势的图 | chart_render：histogram（一列分布）、boxplot 或 bar（按组比较）、scatter（两列相关，可用 fit 画回归线）、line（日期或有序列的趋势）；图片保存为工作区内的 `.png` |

数据在 Excel 里时，请用户另存为 CSV。不要把 read_document 读出的数据手工抄成 CSV：抄错了没人发现，也无法证明它和原文件一致。

## 步骤

1. 用 read_file 看 CSV 的开头，确认列名和哪些列是数值。只看，不心算任何统计量。
2. 要进报告的结果，直接用 statistics_record，result_path 写成 out/ttest.json 这样的相对路径。不要先跑 statistics_ttest 再抄数字。返回里的 keys 就是可以引用的键。
3. 写 Markdown 模板，数字一律写成绑定（写法见下）。
4. 用 report_render 生成报告，再用 report_check 核对。核对列出的每个问题都要告诉用户。
5. 告诉用户生成了哪些文件：报告、绑定清单（报告路径后加 .bindings.json）、结果文件。

## 报告模板

```markdown
# 两组数据的比较

## 方法

用 Welch t 检验比较两组的均值。数字由 R 计算。

## 结果

第一组的样本量为 {{result:out/ttest.json#groups.1.n|%.0f}}，均值为 {{result:out/ttest.json#groups.1.mean|%.2f}}。第二组的样本量为 {{result:out/ttest.json#groups.2.n|%.0f}}，均值为 {{result:out/ttest.json#groups.2.mean|%.2f}}。

两组均值之差为 {{result:out/ttest.json#estimate|%.2f}}，置信区间为 [{{result:out/ttest.json#conf_low|%.2f}}, {{result:out/ttest.json#conf_high|%.2f}}]，p 值为 {{result:out/ttest.json#p_value|%.6f}}。

## 局限

正态性、方差齐性和独立性没有检查。
```

键是数字在结果里的路径：分组用 groups.1.mean 这样的写法，回归系数用 coefficients.2.estimate 这样的写法，编号从 1 开始，对应返回结果里该列表的第几项。格式写 %.Nf（N 位小数），整数写 %.0f。

## 容易踩的坑

正式报告里，除了绑定，**任何阿拉伯数字**都会被 report_check 当作未绑定的数字。常见的有：

- "p 小于 0.05"、"95% 置信区间"、"R 4.6.0"、"第 2 组"、列名里的数字（如 x1）。
- 办法：改用文字（"两组"、"显著性水平"），或者绑定 conf_level 这类结果里的数字，或者用 report_check 的 allow 参数列出确切的正则（如 95 ?%）。allow 只用于你自己写的固定文字，绝不能用来放行计算出来的数字。
- 复述工具返回的警告时，警告里的数字也一样要处理。

p 值很小时，%.4f 会显示成 0.0000，读者会误解。多给几位小数，例如 %.6f。

## 不能绑定的数字

这些数字没有结果文件，写进报告会通不过核对，也不能手工抄：

- statistics_describe 和 statistics_doe 的结果：它们不写结果文件。
- 文字：分组名、方法名。
- 空值：单边检验没有界限的那一端，以及 spearman、kendall 没有的置信区间。

需要它们时，在对话里给出，并说明这些数字没有结果文件，报告核对覆盖不到；不要写进正式报告。只要单列的样本量和均值，可以用 one_sample 的 t 检验；要分组的样本量、均值和标准差，可以用方差分析。

## 交付

- 报告只支持 Markdown。用户要 Word 或 Excel 时，不要把数字复制进 create_document：复制过去的数字不受核对。如实说明经核对的是 Markdown 版本。
- 工具说 R 不可用，或者 R 版本不对，就如实告诉用户并停下，不要自己计算。
- 图表用 chart_render 生成（直方图、箱线图、柱状图、散点图、折线图），由 R 绘制，并附带一份 `delta.figure` 记录。图表是独立的 PNG：可以在 Markdown 报告里用 `![说明](路径)` 引用，不能插入 Word 或 Excel。画图时传入的列名和标题必须来自数据，不要编造。
- 每个结论都要说明前提没有检查，并带上置信区间或 p 值。

## 外部能力

本技能只读工作区里的 CSV，不联网。需要查文献或标准时，用 web_search 和 web_fetch，受工作区的隐私级别约束；被拒绝时如实说明，继续用已有数据完成分析，不要换别的办法绕过。
