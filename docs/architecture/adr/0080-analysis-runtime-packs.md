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
