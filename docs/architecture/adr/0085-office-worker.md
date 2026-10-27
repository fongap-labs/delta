# ADR-0085：Office Worker（成品级 DOCX、PPTX、XLSX）

状态：**已接受**（所有者，2026-10-27，选择 Python 办公库作为生成引擎；蓝图 2.1 "成品标准" 与 "生成引擎" 条款为依据）。

## 背景

- 蓝图 2.1 要求办公输出达到成品标准：模板与样式、图表、版式控制、交付前的结构校验与渲染检查、输出可追溯。
- Delta 现有的原生 OOXML 写入（`document/write`）只保证"能打开、可读"，版式、图表和模板都不具备。它的注释明确把目标限定为"不是设计过的文件"。
- Python 办公库（python-docx、python-pptx、openpyxl）成熟，版式能力足以达到成品标准。Explore Pack 已经验证过"随应用打包的 Python 解释器 + 能力 ABI v2 Worker"的路径，办公能力沿用同一路径。

## 决定

1. **生成引擎是 Python Office Worker。** 生成代码位于 `packages/office_worker/`，依赖 `python-docx==1.2.0`、`python-pptx==1.0.2`、`openpyxl==3.1.5`，以可选依赖组 `office` 的形式固定在 `pyproject.toml` 与 `uv.lock` 中。
2. **权威留在 Rust 核心。** Worker 只把候选文件写入宿主分配的暂存目录，并返回候选声明（路径、大小、哈希）。Rust 负责路径检查、授权、哈希、结构校验、产物登记与台账。Worker 不写入工作区，也不创建正式产物。
3. **工具以结构化内容表达，而不是版式指令。** 调用方提供 `path` 与 `content`（规格），由 Worker 按固定的成品样式排版。规格的字段、类型和上限在 `spec.py` 中统一校验，超限或不合法的规格整体拒绝，不留下半成品。
4. **成品样式是固定的。** 每种格式只有一套样式，写在构建器里：
   - DOCX：统一的字体与标题颜色，表头重复，数字右对齐，页眉与带页码字段的页脚。
   - PPTX：16:9，模板占位符按比例放大（默认模板是 4:3），深蓝标题与顶部色条，深蓝表头。
   - XLSX：深蓝加粗表头、冻结首行、自动筛选、按内容定宽、数字保持数字类型。
5. **交付前重新打开校验。** 每个文件写完后，Worker 用对应库重新打开，核对表格数、标题数、幻灯片数、工作表数与构建器报告的一致。不一致则整个作业失败，候选文件不进入产物登记。
6. **公式安全。** XLSX 中以 `=` 开头的单元格是公式。引用外部工作簿（`[`）以及会访问网络或间接取址的函数（`HYPERLINK`、`WEBSERVICE`、`INDIRECT`、`OFFSET`、`FILEPATH`、`CELL`、`INFO`）一律拒绝。同一工作簿内的跨表引用允许。
7. **启用方式与 Explore 相同。** 只有同时配置 `DELTA_OFFICE_PYTHON` 与 `DELTA_OFFICE_ROOT` 且路径存在时，才注册三个工具（`office_create_docx`、`office_create_pptx`、`office_create_xlsx`）。未配置时不注册任何工具，不回退到其他引擎。
8. **风险与审批。** 三个工具的风险等级为 medium，需要审批，可写工作区；与原生 `create_document` 的约定一致。

## 与原生 `create_document` 的关系

原生 `create_document` 仍保留，行为不变。它是当前可用的最小路径，并在蓝图中标为"最小回退"。切换计划：Office Worker 通过验证并随应用打包后，`create_document` 改为委托给 Worker，之后移除原生 OOXML 写入中与成品生成重复的部分。在此之前，两者并存，模型看到的工具名不同，不会冲突（Rust 测试确认工具名不重复）。

## 实现

- `packages/office_worker/spec.py`：规格校验与上限（文本 32000 字符、单元格 200000、表格 64 列、2000 个块、50 个工作表、200 张幻灯片）。
- `packages/office_worker/docx_build.py`、`pptx_build.py`、`xlsx_build.py`：三个构建器。
- `packages/office_worker/worker.py`：能力 ABI v2 的主循环；校验路径扩展名与格式一致；候选文件命名为 `candidate.<ext>`，因为读取器按扩展名选择格式。
- `crates/delta-core/src/office_worker.rs`：运行时发现、三个工具的注册，以及一个经宿主调用真实 Worker 的端到端测试（需要 `DELTA_OFFICE_PYTHON` 指向装有办公库的解释器）。
- `crates/delta-core/src/core_control_plane.rs`：在 Explore 注册旁边收集 Office 注册。

## 测试

- Python：31 项。覆盖三种格式的内容与样式、规格拒绝（字段名出现在错误信息中）、公式安全、Worker 的协议往返（子进程）、暂存目录校验、扩展名校验、报告与文件不一致时拒绝。
- Rust：5 项，包括经宿主调用真实 Worker 生成 DOCX 并通过校验的端到端测试。
- 全部 `delta-core` 库测试：866 项通过。
- 渲染检查：用 LibreOffice 把生成的 DOCX 与 PPTX 转为 PDF 并截图核对，版式、页眉页脚、表格、16:9 幻灯片标题位置均已修正后确认。XLSX 的公式结果用 LibreOffice 重新计算核对（`SUM` 与跨表引用均正确）。

## 未完成的事项

1. **运行时清单校验（已实现）。** `packages/office_worker/runtime_pack.py` 生成 `manifest.json`，列出 Worker 运行所需每个文件的 SHA-256。Rust 在注册工具前按 `runtime_pack::pack_problems` 逐文件核对（与 Explore 共用同一模块）。
   - 没有清单时严格拒绝，不注册 Office 工具。与 Explore 不同，删除清单不能绕过校验。
   - 开发检出需要显式设置 `DELTA_OFFICE_DEVELOPMENT=1`，此时会在标准错误中记录一行说明。开关只允许"没有清单"的目录通过：有清单的目录始终按清单校验，篡改后即使设置开关也会被拒绝。
   - 清单本身尚未签名（打包阶段处理），与 ADR-0080 记录的已知限制一致。
2. **打包。** 解释器与办公库要随 Delta 安装包一起发布（方案 1），并与 Explore 共用签名流程。体积需要实测。
3. **渲染预览。** 蓝图要求交付前做渲染检查。目前的渲染只在开发时用 LibreOffice 人工完成；把它做成自动检查需要按需安装的 LibreOffice 组件，尚未实现。
4. **模板与品牌。** 目前只有一套固定样式。按项目的模板与品牌色定制，需要模板资产与对应的规格字段，尚未设计。
5. **编辑已有文件。** 本次只做从规格创建新文件。修改已有的 DOCX、PPTX、XLSX（保留原有样式）尚未实现。
6. **图表。** 统计图表（matplotlib + seaborn，遵循 ADR-0080 的绘图约定）尚未接入 Office 的规格，计划在图表工作中补上。
7. **`create_document` 的切换。** 见上文，尚未执行。

## 修订（2026-10-27）：运行时的分发

- 便携版契约禁止把 Python 随安装包发布（`packaging/portable/build-portable.ps1`）。因此 Office 与 Explore 的运行时包（CPython 解释器 + 锁定依赖 + 清单）**在首次使用时下载**，不随安装包携带。
- 解释器版本固定为 **CPython 3.12**。3.12 的安全更新期更长，三个办公库在其上的预编译轮子更稳定；CI 仍在 3.13 上运行，用于提前发现兼容问题。
- 运行时包可以是 ZIP 或 tar.gz 归档（由 `PackSource.archive` 指定）。共享包采用 tar.gz：CPython 的可重定位构建依赖符号链接（如 `python3` 指向 `python3.12`），而 ZIP 路径拒绝符号链接。`crates/delta-core/src/runtime_install.rs` 实现安装：只在出口决定为 `Allow` 时获取；固定 HTTPS 地址；归档 SHA-256 与大小必须与应用内的描述一致；解压到暂存目录，用清单逐文件校验，通过后才原子移入 `<状态目录>/runtimes/<名称>-<版本>/`。任何一步失败都不留下半成品。
- 尚未完成：生产环境的 HTTPS 获取器（需经应用的代理设置）、出口决定的接线（需要用户的出口级别）、运行时包的构建与签名、以及在 external-vault 发布。这些在后续工作中完成。

## 修订（2026-10-27）：批准、接线与共享运行时

- 首次使用时，由用户在应用内一次性确认下载。批准记录写入 `<状态目录>/runtimes/approvals.json`（按包名、版本、归档哈希），并写入审计日志（`audit_events.db`，工具名 `runtime.install`）。拒绝同样写入审计。批准只覆盖确切的包；模型调用的出口规则不变。
- 统计与办公共用一个共享运行时包 `delta-python`（清单格式 `delta.python-runtime`），内含 CPython 3.12、锁定依赖与两个 worker 包。解释器路径由 `PackSource.interpreter` 给出。
- `ExploreRuntime::from_env` 与 `OfficeRuntime::from_env` 先读环境变量（开发用），再由 `register_python_tools` 查已安装且通过清单校验的共享包（`from_installed`）；未安装时不注册任何工具。
- 生产获取器 `HttpsFetch` 经应用的代理设置获取，不跟随重定向，读取上限为已发布的大小。
- tar.gz 解压的安全约束：路径上任一已存在的组成部分若为符号链接（含条目本身），整个条目被拒绝（`refuse_symlinked_path`）；符号链接的目标必须在词法上留在包内。若不做第一条，较早的链接可把后续条目写到包外。回归测试 `a_link_may_not_carry_a_later_entry_out_of_the_pack`。
- 尚未完成：包的构建与签名、在 external-vault 发布，以及把真实描述填入 `runtime/packs.json`（当前为空，即"尚未发布"）。

## 修订（2026-10-27）：界面确认与热注册

- 统计页在没有可用工具时显示 `RuntimePackPrompt`：列出包名、版本、大小与来源域名，用户点击确认后才下载。未发布时只显示"尚未发布"，不提供安装。
- 确认后调用 `CoreControlPlane::install_shared_runtime`：先 `approve_and_install`（记录批准，再按批准安装），成功后用 `register_python_tools` 替换 `statistics_explore_*` 与 `office_create_*` 工具，不需要重启。下载在后台线程进行。
- 状态由 `shared_runtime_status` 给出（是否发布、是否已安装、是否已批准、来源域名）。
- 批准与安装的顺序意味着：批准记录写入后，即使下载失败，同一包不会再次询问。重试会直接下载，失败不留半成品。

## 修订（2026-10-27）：共享包的构建

- 构建器位于 `packages/python_runtime/runtime_pack.py`，入口为 `scripts/build_python_runtime.py`。输入是调用方提供的 CPython 3.12 install_only 归档及其 SHA-256；构建器校验摘要，不自行下载。输出只有两个文件：归档 `delta-python-runtime.tar.gz` 与发布清单 `manifest.json`。
- 归档内的 `manifest.json` 与 `runtime_pack::pack_problems` 的格式一致（`delta.python-runtime`，版本 1），列出除自身外的每个文件（包括符号链接的目标）。发布清单在其之上附加归档的名称、SHA-256 与大小。
- 办公库只取 `uv.lock` 中 `office` 额外依赖的闭包，不包含根项目的其他依赖（如网络搜索与 PDF 库）。环境标记保留给 `uv pip install --python-platform` 按目标平台求值。
- 归档是确定性的：同一输入两次构建的 SHA-256 相同（在 Linux 试构建中已核对）。
- 签名：仅在 CI 中以 cosign keyless 签名（`.bundle` 与归档一同发布）。应用不在安装时校验签名，信任根仍是 `runtime/packs.json` 中固定的 SHA-256。
- 尚未完成：Windows x64 的 CPython 3.12 归档的固定地址与摘要（本次会话无法访问 GitHub 发布资源，需在 CI 中获取并记录）；发布工作流与 Action Worker 中 external-vault 的登记；`runtime/packs.json` 的真实描述。
