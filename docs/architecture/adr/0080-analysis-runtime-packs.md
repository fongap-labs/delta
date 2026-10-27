# ADR-0080：分析运行时包：Explore Pack 与 Validate Pack

状态：**已接受**（所有者，2026-10-27，同意运行时分离、签名环境包与方法包）。

## 背景

- ADR-0079 确定了 `exploratory`、`validated`、`attested` 三档保证等级；还需要决定谁来执行分析、如何固定环境、如何打包与校验。
- 桌面端当前不随附 Python，也不随附 R；R 由用户自行安装。Foundation 的 R 统计与 Suite 的 R 规划会造成两个环境。
- OpenWorker 一类产品证明本地 Python 执行可行，但 Delta 的 Rust Core 必须保留 Workspace、Approval、Ledger、Artifact 与 Job 的唯一权威，不能引入第二套控制面。
- 现有 Foundation 统计把参数校验与结果整形放在 Rust，R 只负责计算。迁移到 Suite 时，这些逻辑不能在 Suite 导入 Foundation 内部实现。

## 决定

### 1. Rust Core 仍是唯一权威

- Workspace、Policy、Approval、Ledger、Artifact、Job 编排与报告门禁由 Rust Core 拥有。
- Python 与 R 都是受控 Worker；它们不拥有 Session、Run、Policy、Approval 或 Artifact 状态。
- 不引入常驻的 FastAPI/Python 应用后端；需要常驻时使用现有 Worker 进程监管机制。
- Foundation 不导入 Suite 实现；Suite 只依赖公开合同。

### 2. Explore Pack

- 使用构建时锁定的 Python 版本；用 `uv` 锁定依赖，随包提供 wheelhouse。
- 独立 Runtime Pack，按需安装，启动时校验版本、锁文件哈希、包签名与平台。
- 默认无网络、只读工作区、只写 staging、CPU/内存/超时限制、包白名单与 SBOM。
- 能力分两层：
  - curated 工具：描述统计、分组比较、相关、回归、图表、数据质量检查；
  - 脚本模式：受控执行 Python 脚本，需审批，记录脚本、环境、输入与输出哈希。
- 只产出 `exploratory` 结果。

### 3. Validate Pack

- R 4.6.0 + `renv.lock` + 版本化方法包（例如 `deltasuite.cmc`、`deltasuite.doe`）。
- 方法包只暴露固定方法；Worker 调用方法包函数，不执行用户提供的 R 代码。
- 每个方法包包含：方法标识与版本、参数 Schema、实现、参考测试、验证报告、适用范围、报告模板。
- 安装时校验签名与哈希；运行前校验 R 版本、包锁与环境摘要。
- 只产出 `validated` 结果，并附验证证据标识。

### 4. Attest Adapters（后期）

- 对接用户已有的 SAS、JMP、Minitab 或 R 环境；只捕获结果与证据，不把外部输出伪装成 Validate Pack。
- 外部结果标记为 `attested`，并记录工具、版本、证据来源与操作者。

### 5. 合同与校验

- 环境清单、方法包清单与分析结果 v2 使用 `schemas/` 中的 JSON Schema。
- 结果记录引擎、环境摘要、方法、输入、参数、警告与限制；报告门禁按 ADR-0079。
- 启动与运行前都做快速校验；校验失败时失败即关闭，不回退到另一个引擎。

### 6. 打包与 CI

- 三个桌面平台各自构建 Runtime Pack；包需要签名，支持离线安装。
- 不依赖用户自行管理的 `pip`、`venv` 或 `renv`；系统安装只作为显式的高级路径。
- CI 固定 Python 锁文件与 R 4.6.0，构建并验证 Runtime Pack 与至少一个方法包。
- Foundation 的 R 统计进入弃用期：先双轨，再迁移，最后移除；旧结果文件保持可读。

## 分发（2026-10-27 决定）

- Explore Runtime Pack 发布到固定仓库 `fongap-labs/external-vault` 的 GitHub Releases，标签 `explore-runtime-v<semver>`；Git 树中不提交归档。该仓库的规范即是把版本化产物放在 Releases 中。
- 归档与清单由 `scripts/build_explore_runtime.py`（实现于 `packages/explore_pack/runtime_pack.py`）生成。归档是确定性的：同一源码两次构建的字节完全一致。清单列出每个文件的 SHA-256 以及归档的 SHA-256。
- 签名采用免费方案：cosign keyless（基于 GitHub OIDC）或 minisign。应用清单固定下载地址、归档 SHA-256 与签名身份。操作系统层面的代码签名（macOS 公证、Windows 证书）不在本决定范围内，暂不做。
- 解释器不随 Pack 分发：要求系统 Python ≥3.11，并通过 `DELTA_EXPLORE_PYTHON` 显式指定。
- 运行时校验：若 `DELTA_EXPLORE_ROOT` 指向的目录含有 `manifest.json`，Rust 在注册工具前逐文件核对 SHA-256。任何不一致、缺失或越界的路径都会使工具不注册（失败即关闭）。没有清单的目录视为开发检出，不做此校验。
- 已知限制：清单本身在签名接入前不受保护；删除清单会使目录退回开发模式。签名步骤与 external-vault 的发布规则需要在源仓库和 Action Worker 中配置，尚未完成。
- 发布声明（尚未提交）：Action Worker 的根级发布清单 `.github/release.manifest.json` 已属于 Delta 产品，且一个仓库只能采用一种布局（根级单产品，或 `projects/<product>/` 多产品）。因此 Explore Runtime 的清单在布局决定之前不提交，不覆盖 Delta 的清单。构建脚本为 `.github/scripts/build-explore-runtime.ps1`（Action Worker 要求 `.ps1`），计划在 `linux-standard` 运行器上调用 `scripts/build_explore_runtime.py`。版本只来自发布时的手动输入，构建会拒绝与 `PACK_VERSION` 不一致的版本。资产固定为 `explore-runtime.tar.gz` 与 `manifest.json`，名称中不含版本。
- 尚未完成：发布清单的布局决定与提交、外部目标仓库策略（`external-vault` 未登记为发布目标）、运行器上的 `pwsh` 实测、签名步骤，以及 `external-vault` 中的工具元数据。这些需要维护者配置或后续阶段完成。

## 后果

- 新增两类可分发的运行时包与一套方法包合同。
- Foundation 的分析能力不再依赖用户安装 R；但需要新的 Python 运行时打包、签名与安全检查。
- Suite 的方法包需要独立验证与发布流程。
- 现有 Foundation R 统计需要迁移计划与弃用公告；不能直接删除。

## 未验证事项

- Python Runtime Pack 的具体分发方式（内嵌解释器、独立 pack 或按需下载）尚未定案。
- 签名基础设施与三平台构建尚未实现。
- Suite 仓库当前不在本会话的访问范围内；方法包与 R Worker 的现状需要在获得权限后核对。
- 验证证据的格式与首批方法尚未确定。

## 实现记录（2026-10-27）

- Foundation 的 R 统计、R CI 与 external-vault R 资产已立即移除（ADR-0081）。
- Explore Pack 第一阶段（`packages/explore_pack/`）：标准库实现的描述统计、Welch t 检验、Pearson 相关与简单回归；CLI 写出 `delta.result` v2，`assurance` 固定为 `exploratory`，并以 `analysis-result.schema.json` 校验。依赖只新增开发期的 `jsonschema`，运行时不引入第三方包。
- 尚未实现：Rust 能力注册与审批接入、受控脚本模式、独立 Runtime Pack 的签名与三平台构建、统计页接入。这些是 P3 的后续工作。
- Suite Validate Pack 的 R 方法包由 Suite 仓库另行交付，不在本次范围内。
- 第二阶段：`packages/explore_pack/worker.py` 按能力 ABI v2 从 stdin 读取作业、向 stdout 写出结果；Rust 侧 `statistics_explore` 在配置 `DELTA_EXPLORE_PYTHON` 与 `DELTA_EXPLORE_ROOT` 后注册四个 `statistics.explore.*` 工具，统计页可在无模型时运行它们（ADR-0078 修订）。未配置时不注册任何工具。
- 第二阶段仍未解决：Runtime Pack 的签名接入与发布（见上方"分发"一节）；受控脚本模式。
- 统计运行的审计条目已在 `CoreControlPlane::audit_statistics_run` 实现（写入 `audit_events.db`，写入失败即拒绝运行），此前的待办已过时。

## 修订（2026-10-27）：解释器的分发

- 上文"解释器不随 Pack 分发"已被修订：统计与办公的运行时包包含可重定位的 CPython 3.12 解释器，并在首次使用时从固定 HTTPS 地址下载、校验后安装（见 ADR-0085 修订）。`DELTA_EXPLORE_PYTHON` 仍可用于开发检出。
- 解释器不随安装包发布，以符合便携版的发布契约。
