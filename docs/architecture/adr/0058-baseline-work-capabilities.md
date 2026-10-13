# ADR-0058：基础办公能力归属 Foundation

状态：**已接受**（所有者，2026-10-11）。实现分步进行；每一步落地时都记入 CHANGELOG。

## 背景

蓝图列出三类工作：日常办公、研究与内容创作。ADR-0002 与 `REPOSITORY_BOUNDARY.md` 规定，Foundation 必须能够在未安装任何扩展的情况下运行并完成"基础工作"，而专业能力则是可选的扩展。

Foundation 当前的实际情况（在 `CapabilityHost::product_defaults`，即 `crates/delta-core/src/capability/host.rs`，及其测试 `product_registry_owns_stable_model_tool_schemas` 中核对）：模型获得五个工具：`ask_user`、`propose_plan`、`read_file`、`request_directory` 与 `write_file`。它无法读取 PDF、Word 或 Excel 文件，无法抓取网页，无法搜索网络，也无法创建 Office 文件。`integrations/` 下有这些工具的 Python 版本，但桌面应用不随附 Python，Rust 运行时也不调用它们，因此它们在产品中不可用。

因此，只安装 Foundation 的用户无法完成蓝图承诺的第一件事：读取文档、研究问题、产出交付物。这与"Foundation 是一个完整、可独立运行的产品"相冲突。

## 方案

**A. 将读写常见办公文件以及读取网页，作为基础能力，在 Foundation 中以原生方式实现（已选定）。**
- 优点：安装后即可使用；无需解释器；只有一条 Trust 路径；无需安装或保持同步任何东西。
- 缺点：Foundation 需要引入解析器 / 写入器依赖与网络代码，它们都必须经过审查；蓝图中"不做重型 Office 套件"的限制必须通过范围来保证，而不是通过所在位置来保证。

**B. 作为扩展保留，以 Python Worker 形式发布。**
- 优点：与对 ADR-0002 的早期字面理解一致。
- 缺点：产品中没有 Python，因此在用户安装运行时和扩展之前，基础能力为空。

## 决定：采用 A，并严格限定"基础"的含义

Foundation 以原生能力的形式，在能力宿主下提供以下工具，其授权、审批、Ledger 与产出物规则，与 `read_file` 和 `write_file` 相同：

| 工具 | 功能 | 风险与审批 |
|---|---|---|
| `read_document` | 从工作区的 PDF、DOCX、XLSX 与 PPTX 文件中提取文本与结构 | 低风险，无需审批（与 `read_file` 一样读取；计为一次来源读取） |
| `web_fetch` | 抓取一个页面，输出为可读文本 | 中风险，需要审批（对外流量；URL 由模型选择） |
| `web_search` | 针对一个查询，返回标题、URL 与摘要 | 中风险，需要审批（对外流量；查询由模型选择） |
| `create_document` | 根据结构化内容创建一个新的 DOCX、XLSX 或 PPTX，并回读检查 | 中风险，需要审批；与 `write_file` 一样作为产出物暂存 |

使这些能力保持在蓝图范围之内的规则：

1. **范围。** 读取，以及根据结构化内容创建文件。不包括原地编辑、版式设计、宏，或与 Office 兼容的编辑器（蓝图 2.1："不是完整的 Office 套件"）。
2. **网络。** `web_fetch` 与 `web_search` 拒绝回环地址、私有地址、链路本地地址（包括云元数据地址）、CGNAT、组播与保留地址，包括内嵌在 IPv6 中的 IPv4 地址，并且在每一跳重定向时都进行检查。它们只连接到通过检查的那个地址，因此一个会改变解析结果的名称无法把连接重定向。它们将响应体限制为解压后 5 MB，读取超时为 30 秒，并把外部文本作为数据返回，绝不作为指令。这是 Python `integrations/web/guard.py` 中策略的移植，而非放宽。
3. **校验。** `create_document` 使用与 `read_document` 相同的解析器回读文件，如果所要求的结构不存在，则失败。绝不因为模型的一句话就报告文档已创建（蓝图 4.4）。
4. **依赖。** 只允许纯 Rust 实现，每项依赖都须受仓库的 cargo-deny 策略覆盖，并且各自服务于 Work 与 Capability 模块：ZIP/XML 读取器（`zip`、`quick-xml`）与 PDF 文本读取器（`lopdf`）；DOCX、XLSX 与 PPTX 写入器直接基于 `zip` 构建包，不再引入其他依赖。`pdf-extract` 与较旧版本的 `lopdf` 被否决，因为它们会引入 `ttf-parser`，而该库在安全公告数据库中被列为无人维护，且 `deny.toml` 不允许。不依赖系统库，也不依赖解释器。
5. **扩展依然存在。** 专业能力（统计与实验设计、图像与视频、连接器）仍是可选扩展。扩展可以通过注册另一个名称的工具来替代某个基础工具；Foundation 不点名任何扩展。

## 后果

- `REPOSITORY_BOUNDARY.md` 无需修改："通用基础能力"在本 ADR 中有了确定的含义。
- Python 的 `integrations/tools` 与 `integrations/web` 代码是移植的行为参照。只有在无人调用它们时，才按 `DEPENDENCY_POLICY.md` 的要求将其移除。
- 新工具不需要新的审批路径：`web_fetch` 与 `web_search` 已在 `policy.rs` 中被归类为对外流量（egress）。
- 搜索使用用户排序的引擎列表：Google、Bing 与 DuckDuckGo 内置，无需密钥；Brave 与 Tavily 需要密钥；用户还可以添加引擎（一个带 `%s` 的 URL）。该列表以及每个密钥，都是同一个凭据存储值，且只授予搜索工具。

## 未验证事项

生成的 DOCX、XLSX 与 PPTX 文件是用 python-docx、openpyxl 与 python-pptx 打开的，并用 LibreOffice 渲染，未使用 Microsoft Office 打开。PDF 文本的输出为每行一个文本段。

网页与搜索结果的解析是依据撰写时所见的标记编写的，随着服务商修改页面，需要持续维护；搜索解析器是针对保存的样本测试的，并未针对线上真实站点测试。
