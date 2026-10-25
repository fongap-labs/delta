# ADR-0054：哪个统计引擎产出可报告的数字

状态：**已接受**（所有者，2026-10-11），同日**修订**：R 由 Rust 原生能力直接调用，而不经 Python Worker（见"修订（2026-10-11）"）；2026-10-12 再次修订，R 版本锁定为 4.6.0（见"修订（2026-10-12）"）。

**部分实现**：已实现 `statistics_describe`（描述统计）以及 2026-10-25 的六个方法（见文末"实现记录（2026-10-25）"），均仅由 R 4.6.0 计算。NIST StRD 参考测试、`renv.lock` 与容器镜像摘要尚未完成。

## 背景

蓝图把研究分析列为 Delta 的三个产品域之一（`DELTA_BLUEPRINT.md` 第 2.2 节）：描述统计、统计推断、回归、DOE、序贯设计与正式报告。第一个目标域是制剂 / CMC 研究分析，以扩展的形式在 `fongap-labs/delta-suite` 中交付；Foundation 保留通用的统计能力（所有者，2026-10-11）。

撰写本 ADR 时，Delta 尚无统计能力。现存情况如下（已核对，并非假设）：

1. 能力 ABI 通过 `WorkerProcessRunner`（`crates/delta-core/src/capability/runners.rs`）运行进程型 Worker，它接受任意程序：Python、PowerShell 与 Shell 有辅助函数，"其他受控 Worker"则使用 `WorkerProcessRunner::new(program, arguments)`。
2. 唯一的 Worker SDK 是 Python 的 `delta-worker-sdk`（`packages/delta_worker_sdk`）。
3. 蓝图在任务执行语言中列出了 Python、PowerShell 与 Shell（第 8.2 节）。R 未被列出。
4. 桌面应用不随附 Python。`packaging/portable/build-portable.ps1` 声明 Python 不是应用依赖；ADR-0058 记录 Rust 运行时不调用 `integrations/` 下的 Python 工具。基础能力（文档、网页）是 `CapabilityHost::product_defaults` 中注册的 Rust 原生能力。
5. 没有统计能力，模型就会自行计算数字，或凭记忆给出数字。对制剂 / CMC 用户（稳定性货架期、溶出 f2、DOE）而言，若一个数字不是由确定性、带版本的引擎产出的，就不可用。

所有者要求统计技术栈使用监管机构认可的库（2026-10-11）。这需要说得更精确，因为没有任何监管机构公布过经批准的统计库清单：

- FDA 的《统计软件澄清声明》（*Statistical Software Clarifying Statement*）称，FDA 不要求任何特定的统计软件；申办方负责记录所用的软件，并证明其适用于预期用途。据我们所知，EMA 与 NMPA 也不认证统计软件。
- 监管机构在实践中关注的是**有记录的验证与预期用途适用性**，以及递交资料中的先例。以此衡量：
  - **SAS** 的递交历史最长（主要是临床领域）。
  - **R** 有公开的合规依据：R 基金会的 *R: Regulatory Compliance and Validation Issues — A Guidance Document for the Use of R in Regulated Clinical Trial Environments*，R Validation Hub 基于风险的软件包评估（`riskmetric`），以及 R Consortium 用 R 完成、提交给 FDA 的试点案例。
  - **JMP 与 Minitab** 是 CMC 与制剂科学家在 DOE、稳定性和能力分析中最常用的工具。
  - **Python**（SciPy、statsmodels）在数值上是可靠的，但在软件包层面没有可比的监管验证指南或递交先例。

上述关于监管声明与试点的事实，来自撰写时已知的公开文件。在任何面向用户或验证材料中引用之前，应与最新的官方原文核对。

## 方案

**A. 仅 Python（NumPy、SciPy、statsmodels），置于现有的 Python Worker SDK 之内。**
- 优点：无需新的运行时；一个 SDK；打包简单。
- 缺点：不满足所有者的要求。没有可以引用的监管验证指南或递交先例，因此每位用户都必须自行论证该技术栈。

**B. 以 R 为可报告数字的引擎，由 Python Worker 驱动（推荐）。**
- 基于 `delta-worker-sdk` 的 Python Worker 接收作业、准备输入、用 `Rscript` 运行固定的 R 脚本，并通过现有 ABI 返回结果。
- 仅使用 Base R（`stats`）以及一小组固定版本的 CRAN 软件包，计算每一个可能进入正式报告的数字。
- 优点：以有记录的合规依据满足要求；保持一个 Worker SDK 与现有 ABI；R 仅作为计算引擎，而不是权威。
- 缺点：将 R 增加为任务执行语言；需要为打包引入 R；软件包版本必须固定，并经过风险评估。

**B2. 以 R 为引擎，由 Rust 原生能力直接调用（由修订采纳）。**
- 注册在 `CapabilityHost::product_defaults` 中的能力，通过 `WorkerProcessRunner` 运行固定的 R 入口脚本（`Rscript`）。`WorkerProcessRunner` 已经能在运行时的超时与取消机制下运行任意程序。
- 优点：用户只需 R，无需 R 与 Python 两者；运行时与计算之间少一个进程；取消直接作用于 R 进程；验证包只需覆盖 R。
- 缺点：需要一个小型的 R 端接口来传递输入与结果；Rust 负责准备输入表（自 ADR-0058 起，它已经能够读取 XLSX、CSV 与 DOCX）。

**C. 驱动用户自己已验证的 JMP、Minitab 或 SAS 安装。**
- 优点：使用用户已经验证过的工具。
- 缺点：商业许可；各工具、各平台的脚本与自动化方式各不相同；Delta 无法随附或针对它们进行测试。这更适合作为 Suite 中后续的导出或适配器（例如输出 JMP 脚本），而不是作为引擎。

## 决定：B2（由 B 修订而来），并可选导出 JMP / SAS 脚本

1. **引擎。** 任何可能出现在正式 Artifact（报告、表格、图注）中的数字，都由 R 计算。Rust 只能读取、重塑与校验输入数据，并渲染结果；它不计算可报告的统计数字。
2. **形态。** Foundation 能力为 `statistics.*`（例如 `statistics.describe`、`statistics.regression`、`statistics.anova`、`statistics.ancova`）。它是一个 Rust 原生能力，通过 `Rscript` 运行编译进二进制文件的 R 脚本，因此每次构建中的 R 脚本 SHA-256 是固定的。R 进程是能力 ABI 下的受控 Worker：限定范围的输入、暂存的输出、超时与取消，且不拥有任何权限。
   - **接口仅使用 Base R。** Rust 将输入表写为 CSV，将参数写为 DCF 文件（`read.dcf`，Base R）。R 脚本将结果写为 DCF，每个数字都经由 `sprintf("%.17g")` 输出，使双精度数可以精确往返。无需 JSON 包，因此 Foundation 仅依赖 Base R 与推荐包（第 4 点）。
3. **固定环境。**
   - R 版本与每个软件包的版本都要固定。
   - R 版本为构建 Worker 时 CRAN 发布为当前版本的 R 发行版，并固定到确切的 `x.y.z`。
   - 升级到更新的 R 发行版属于有意识的更新：锁文件随之变化，参考测试（第 6 点）须在新版本上通过，CHANGELOG 中同时写明两个版本。
   - 软件包锁随 Worker 一同提供，例如 `renv.lock` 或带日期的仓库快照。
   - 启动时，Worker 检查已安装的环境是否与锁文件一致。若不一致，则以类型化错误失败即关闭；绝不在未固定的环境中运行。
4. **软件包准入。**
   - Foundation 只使用 Base R 与推荐包。
   - 其他每个软件包在准入之前，都需要记录在案的风险评估（R Validation Hub 方法），以及对照 `DEPENDENCY_POLICY.md` 的许可证检查。
   - 领域软件包（例如 bootstrap f2 的实现）在 Suite 中准入并固定，不在 Foundation 中。
5. **每个结果都附带来源证明（provenance）。** 结果载荷包含：
   - R 版本；
   - 实际加载的软件包名称与版本；
   - R 脚本与每个输入文件的 SHA-256；
   - 方法名称及其参数。

   这正是 [ADR-0055](0055-numeric-traceability.md) 用来把报告数字绑定到计算结果的依据。
6. **验证。** 每个 `statistics.*` 函数在发布之前都必须通过参考测试：
   - 用 NIST 统计参考数据集（StRD）检验数值精度（线性回归、ANOVA）；
   - 已发表指南中的示例；
   - 测试套件中与 statsmodels / SciPy 的独立交叉核对。

   超出既定容差的偏差会使发布失败。
7. **可用性。** 首个版本的便携版不捆绑 R。该能力检测受支持的 R 安装；若未找到，它会报告自身不可用并给出明确原因，且绝不回退到其他引擎。是否捆绑 R 是单独的打包决定（涉及体积以及再分发二进制时的 GPL 许可义务）。
8. **蓝图。** 第 8.2 节将 R 加入任务执行语言，并遵守相同的 Worker 规则：限定范围的输入与输出、超时、最小权限、无权限。
9. **可选的 JMP / SAS 脚本导出（选项 C，作为导出而非引擎）。** Suite 适配器可以在结果旁边写出 JMP（JSL）或 SAS 脚本，在同一输入文件上复现相同的分析。
   - 该功能是可选的，除非用户要求，否则默认关闭。
   - 它只写出脚本成果；从不运行 JMP 或 SAS，也绝不把它们的输出回馈到 Delta 报告中。
   - Delta 中可报告的数字始终来自 R 引擎。导出的作用是让用户能够在其组织已验证的工具中重新运行分析，并比对结果。

## 后果

- `delta-core` 新增 `statistics` 模块（注册、输入准备、结果解析）及其 R 脚本，与文档、网页能力一同注册在 `CapabilityHost::product_defaults` 中。
- ABI 与 Python Worker SDK 不变。`WorkerProcessRunner` 直接运行 `Rscript`，并处于作业的超时与取消机制之下。
- 验证包随能力一同增长：
  - 软件包风险评估；
  - 每个版本的参考测试结果；
  - 环境锁文件。

  这些材料是用户的 QA 判断预期用途适用性所需的依据。Delta 不声称该能力"已验证"或"已获监管机构接受"。
- CI 需要在执行统计测试的 Runner 上安装 R 工具链（中央 CI 位于 Action Worker）。

## 所有者的答复（2026-10-11）

1. 接受 B：R 负责计算，Python 负责编排、数据处理与渲染。
2. 可选的 JMP / SAS 脚本导出计划在 Suite 中实现（决定第 9 点）。
3. 固定当前的 R 发行版，而非上一个次版本（决定第 3 点）。
4. 修订：R 由 Rust 原生能力直接调用，而不经 Python Worker，因为桌面应用不随附 Python（选项 B2）。

## 修订（2026-10-11）

第一版决定（B）让 Python Worker 驱动 R。桌面应用不随附 Python（背景第 4 点），因此 B 会要求每位用户同时安装 Python 与 R 才能使用统计功能，并且会在运行时与 R 之间插入一个 Python 进程。所有者选择了 B2。本 ADR 的其余内容保持不变：R 计算每一个可报告的数字；环境被固定并检查；每个结果都附带来源证明；参考测试对发布起门禁作用。Python（statsmodels）仅保留在测试套件中，作为第 6 点的独立交叉核对。

## 修订（2026-10-12）：锁定 R 4.6.0

所有者将 R 发行版锁定为 **4.6.0**（确切版本）。这固定了决定第 3 点：

- 统计仅由 R 4.6.0 计算。该能力读取 `Rscript` 报告的版本，并拒绝任何其他发行版，因此绝不会在未经验证的环境中计算（`statistics::PINNED_R_VERSION`）。
- 升级到其他发行版属于受控变更：需要新的决定、在新版本上通过参考测试，并在 CHANGELOG 中写明两个版本。
- 交付遵循所有者描述的受控环境做法（2026-10-12）：版本、软件包锁与系统库一同冻结。CI、验证环境与服务器部署使用按摘要（digest）固定的容器镜像；桌面应用运行在用户自己的电脑上，不假定有 Docker，它使用 R 4.6.0 的安装，并在启动时按同一锁文件进行检查。两者每 12 个月审查一次，新发行版只有在参考测试与数值溯源检查在其上通过之后，才会被采用。
- 确切的镜像摘要、软件包锁（`renv.lock`）以及 NIST StRD 参考结果尚未纳入仓库；它们是下一步工作。

## 未验证事项

- 关于 FDA、EMA、NMPA 与 R Consortium 试点的陈述，来自撰写时已知的公开文件，未在撰写本 ADR 时重新核对。
- 撰写本 ADR 时尚未构建 R 能力。截至 CHANGELOG 最新条目，`statistics_describe` 已经落地，但 CI 尚未安装 R 4.6.0；NIST 参考测试与 `renv.lock` 也尚未完成。
- 在 Windows 上，`Rscript.exe` 会把 R 引擎作为另一个进程启动；取消操作必须结束整个进程树（例如通过作业对象，与受控浏览器的做法相同），并且必须经过测试。

## 实现记录（2026-10-25）

按所有者对路线图 B2 的批准，Foundation 新增六个 `statistics.*` 方法，形态与 `describe` 相同（决定第 2 点）：

| 工具 | 方法（全部是 Base R `stats`） |
|---|---|
| `statistics_ttest` | `t.test`：单样本、两组（Welch 或合并方差）、配对；可选单侧与置信水平 |
| `statistics_anova` | `aov`：单因素方差分析，每组 n、均值、标准差，平方和、F、p |
| `statistics_chisq` | `chisq.test`：两个分类列的独立性检验，返回最小期望频数 |
| `statistics_correlation` | `cor.test`：Pearson、Spearman、Kendall |
| `statistics_regression` | `lm`：最多 8 个数值预测变量，可加全部两两交互；共线的系数返回 null 并列入 `aliased_terms` |
| `statistics_doe` | 两水平全因子设计：重复、中心点、由种子复现的随机运行顺序 |

设计要点：

- **参数不是代码。** 列名、分组标签等只作为命令行参数传给固定的 R 程序，回归的模型公式由 R 程序用 `x1`、`x2` 等安全名字构造，再由 Rust 换回用户的列名。任何列名都不会进入 R 源码。
- **来自数据的文字经百分号编码。** 分组标签、检验名称与 R 的警告由 R 编码后输出，Rust 解码，所以标签中的冒号、换行或非 ASCII 字符不会破坏 `key: value` 格式；分组按字节顺序排序，与区域设置无关。
- **警告随结果返回。** 例如"卡方近似可能不准确"、"有结值，无法计算精确 p 值"，最多 5 条。工具说明与系统提示词要求模型转述警告，并说明未检查的前提（正态性、方差齐性、独立性）。
- **来源证明。** 结果的 `provenance` 记录 R 版本、加载的软件包（`base`、`stats`、`utils`）、完整 R 程序（公共前导加方法程序）的 SHA-256、输入文件的 SHA-256、方法名与参数（DOE 含种子）。
- **R 的 `NA`、`NaN`、`Inf` 在 JSON 中为 null。** 单侧检验的置信区间有一端无界，该端为 null。
- **DOE 的范围。** 只提供两水平全因子设计，最多 8 个因子、1024 次运行；不提供部分因子、响应曲面与最优设计（需要非 Base R 软件包，按决定第 4 点应在 Suite 中经风险评估后准入）。设计表只是确定性的组合，随机顺序用 R 的 `set.seed` 与 `sample` 生成，由种子复现。
- **没有写文件，也不需要审批。** 方法只读取工作区内的 CSV，副作用为 `read`；DOE 无输入，副作用为 `none`。

验证（决定第 6 点）：每个方法的测试用 SciPy 1.18 与 statsmodels 0.15 独立算出的数值检验 R 的输出（t 检验、方差分析、卡方、相关、回归及带交互的回归，相对误差一般在 1e-9），并检验错误输入、警告与来源证明。这些测试在 R 4.3.3 上运行通过；测试绕过版本锁定，因为数值不依赖 R 的发行版，而产品本身仍只接受 R 4.6.0。

## 实现记录的未验证事项

- **没有在 R 4.6.0 上运行。** 本次开发环境无法下载 R 4.6.0 的源码包，测试只在 R 4.3.3 上通过。中央 CI 会构建 R 4.6.0 并运行同一批测试；若有数值或警告文字的差异，以 CI 结果为准。
- Spearman 的 p 值，R 在无结值时用精确或 Edgeworth 近似，SciPy 用 t 近似，二者不同；测试只断言它远小于 1e-4，不断言相等。
- NIST StRD 参考数据集与已发表指南中的示例仍未纳入，决定第 6 点只部分满足。
- 在 Windows 上，非 ASCII 的列名经命令行传给 `Rscript.exe` 的行为未测试；Linux 下在 `C`、`POSIX`、`C.UTF-8` 三种区域设置中通过。
- 路线图中"`study.json` 序贯设计"没有实现。
