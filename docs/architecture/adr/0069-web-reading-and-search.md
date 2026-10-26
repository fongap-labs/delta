# ADR-0069：网页读取与搜索的改进

状态：**已接受**（所有者，2026-10-26）。承接 ADR-0058、ADR-0060、ADR-0061，不改变它们的审批与出境规则。

## 背景

所有者反馈：Delta 的网页搜索和网页读取明显落后。对 `crates/delta-core/src/web/` 的核查（已核对代码，并非假设）：

1. **读取。** `web_fetch` 用 `html_to_text` 只剥标签：导航栏、页脚、cookie 提示和侧栏与正文一起占满 2 万字符的额度；所有 `href` 丢失，模型无法顺着链接继续读；标题、列表、表格、代码块被压平；超过上限的页面后半部分读不到（没有续读）。
2. **文档。** 网址指向 PDF 或 Office 文件时直接报错，提示"下载后用 `read_document`"，但并没有下载工具，所以网上的 PDF（论文、报告）一律读不了。这对研究分析是硬缺口。
3. **脚本页面。** 页面靠脚本生成内容时返回近乎空白，模型不知道原因。
4. **请求。** 标识为 `Delta/x (+desktop)`，不少站点直接拒绝；服务器返回 429/503 时不重试。
5. **搜索。** 默认引擎（Google、Bing、DuckDuckGo）读公开结果页，易受改版和拦截影响；没有时间范围和站点过滤；结果没有日期；同一页面带不同跟踪参数会重复出现。

## 决定

### 读取（`web_fetch`）

- HTML 转为 Markdown（`web/markdown.rs`）：保留标题、列表、链接（改成绝对地址）、表格、代码块（含语言）、引用，图片只保留有意义的 alt；去掉导航、页脚、侧栏、cookie 与弹窗、隐藏元素。
- 先只取 `<main>`（没有则 `<article>`）；结果过短，或去除页面杂项后只剩原文很小一部分时，退回整页，所以被误认的"菜单"不会让页面变空。
- 新参数 `offset` 与返回的 `next_offset`、`total_chars` 用于续读；`links: true` 返回页面链接（文字与绝对地址，最多 100 条），模型自己决定要不要继续读。
- 网址指向 PDF、DOCX、XLSX、PPTX 时，用 `read_document` 同一个解析器直接读出（上限 25 MB；页面仍是 5 MB）。
- 内容几乎为空且像脚本外壳时，返回 `note`，建议改用 `browser_navigate`。
- 标识改为 `Mozilla/5.0 (compatible; Delta/x; +desktop)` 并带 `Accept-Language`；429、502、503、504 与超时重试一次。

### 搜索（`web_search`）

- 新参数 `recency`（day、week、month、year）与 `site`：各引擎用自己的方式表达（Google `tbs`/`dateRestrict`，DuckDuckGo `df`，Brave `freshness`，Tavily `time_range`/`include_domains`，`site:` 操作符用于其余引擎）。引擎不支持时只在 `note` 里如实说明，不报错。
- 结果去掉 `utm_*` 等跟踪参数和 `#片段`，同一页面只列一次；返回 `domain`，引擎给出日期时返回 `date`（Brave、Tavily 与常见 JSON 字段）。

## 不做

- **爬虫工具。** 不新增整站抓取。模型用 `links` 加 `web_fetch` 逐页读取，每次仍走审批、出境档位和地址守卫（ADR-0058、ADR-0061），不会因为"爬"而绕开用户可见的批准。
- **脚本渲染。** `web_fetch` 不执行脚本；需要时用受控浏览器（ADR-0060）。
- **多引擎结果合并。** 额外请求会增加对外流量，收益未经证明。

## 后果

- `web_fetch` 的文本格式从纯文本变为 Markdown，输出多了 `total_chars`、`next_offset`，可选 `links`、`note`。
- `fetch::Limits` 增加 `max_document_bytes` 字段。

## 未验证事项

- **开发环境无法联网**，没有在真实网站和真实搜索引擎上验证。抽取器用手写的样本页面测试，搜索用样本结果页和请求地址断言。真实站点上的抽取质量需要用一批常见站点（新闻、文档、论坛、维基）抽样检查。
- `recency` 在 Google（`tbs=qdr:`）、DuckDuckGo（`df=`）、Brave（`freshness=p?`）、Tavily（`time_range`）上的参数来自这些服务的常见用法；Bing 页面的 `filters=ex1:"ez1/2/3"` 窗口编码未对照当前页面确认，不生效时只是结果不受时间限制。
- 默认的免密钥引擎仍然是抓取结果页，稳定性取决于对方。要稳定的结果，建议在设置里配置 Brave 或 Tavily 的密钥。
