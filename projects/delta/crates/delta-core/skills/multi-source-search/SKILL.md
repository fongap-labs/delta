---
name: multi-source-search
description: 多源检索：把一个问题拆成检索主题，同时在工作区（conversation / library / grep）和对外（web_search / scholar_search / connector 搜索）并行检索，去重、摘录、标注出处，交付可复用的检索记录。
source: builtin
version: 1
side_effect: read
capabilities: ["web_search", "web_fetch", "scholar_search", "scholar_paper", "scholar_cite", "conversation_search", "library_search", "library_index"]
---

# 多源检索

一个问题的答案往往分散在多个来源：你之前的对话、你工作区的文件、代码仓库、网页、学术论文。这个技能把"本地（对话/文件/代码）+ 对外（Web/学术/SaaS 连接器）"的多源并行检索固化成可复用流程，适合"调研某个话题"、"核实一个说法"、"找一段之前说过的话"这类任务。

你做不了的事要直说：不能保证检索穷尽，不能读付费墙后的内容，不能把模型自己的记忆当作来源，也不能代替用户做判断。你交付的是一份检索记录、去重后的来源列表和每条来源的简短摘录。

## 开始前先问清（一次问完）

用 ask_user 一次问清：调研的问题或说法；要不要只在本地工作区找（conversation / library / grep），还是也可以上网；参考文献格式（bibtex、apa 或 gbt7714）；大概要多少条来源。已经说过的不要再问。

## 步骤

1. 把问题拆成 2 到 4 个检索主题，每个主题写出 2 到 3 组检索词（中英文各一组）。如果用户只想在本地找，跳过第 2、4、5 步中的网络和学术步骤。
2. 本地并行检索：
   - conversation_search：查这段话是否在以前的会话里说过过；每次记下检索词和命中的会话与行。
   - library_search：查用户工作区的文件（PDF / Word / Excel / PowerPoint / 文本）；每次记下检索词和命中的文件与位置。
   - grep（代码/配置搜索）：在仓库源码或配置里找关键词。
   把命中归并到同一主题下，一条来源只列一次，保留文件路径、行/页、片段。
3. 对外网络检索（用户允许时）：对每个主题用 web_search 检索，先用内置的 keyless 引擎（DuckDuckGo / SearXNG 实例），失败或空结果时自动回退下一个；每次记下检索词、引擎和日期。
4. 学术检索（用户允许且涉及论文时）：对每个主题用 scholar_search 检索；入选的用 scholar_paper 读记录，顺 references / cited_by 补关键论文；需要全文时用 web_fetch 读 open_access_url，没有免费全文或只读了摘要的，写明“仅依据摘要”。
5. 核验：把准备引用的每一条交给 scholar_cite（学术）或直接核对标题与 URL（网页）；found 为 false 或 title_matches 为 false 的不要引用，并告诉用户为什么。引文只用检索返回的信息，不要凭记忆写参考文献或 DOI。
6. 归并与去重：同一来源在多个主题或多个引擎里命中的，只保留一条，并标出所有命中的检索词。按“本地来源优先、网络来源次之”排序。
7. 写出检索记录：`search-log.md`，包含检索词、年份/时间范围、日期、入选与排除的来源及理由、数据来源（对外搜索用了哪些引擎/实例；本地用了 conversation / library / 哪个文件）。
8. 交付：一份带出处的来源列表，每条带标题、URL/路径、片段、检索主题；再按主题写一段简短的调研小结，区分“来源说了什么”和“你的判断”。

## 外部能力

web_search / scholar_search / web_fetch 会把检索词发到公开服务，受工作区的隐私级别约束，被拒绝时如实说明，不要换别的办法绕过。本地的 conversation_search / library_search / grep 完全在本机，不外传。
