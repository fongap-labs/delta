# 可选扩展的加载方式（设计决定）

状态：提议（Proposed），与 Delta Foundation 与 Delta Suite 的现有实现一致；待所有者接受。

本文记录 Delta 如何发现并激活可选扩展，比较备选方案，并列出决定之后仍需单独处理的问题。

## 1. 问题

仓库里出现过两种加载说法，互相矛盾：

- `contracts/README.md` 与 `scripts/check_architecture_boundary.py` 曾把 Python `delta.extensions` entry-point 组当作扩展的加载途径；
- Delta Suite 的 `scripts/check_boundaries.py` 禁止 `[project.entry-points."delta.extensions"]`，其 `docs/ARCHITECTURE_BOUNDARY.md` 写明"Python 自动发现与包入口点都不是 Runtime 激活机制"。

Foundation 仓库中并不存在 entry-point 加载代码（见 `docs/architecture/repository-layout.md`）。实际运行的机制已经是清单加载。

## 2. 现状（以代码为准）

- Rust Runtime 扫描 `<state>/extensions/<source>/`：
  - `capability-workers.json`（`crates/delta-core/src/extension_manifest.rs`）：校验 `version`（当前为 1）、`source` 必须与目录名相同、`program` 必须是绝对路径；每个扩展独立加载，坏清单只记录错误并跳过，不阻止 Foundation 或其他扩展启动；
  - `connector-catalog.json`（`crates/delta-core/src/application.rs`）：同样校验 `version`（当前为 1）与 `source`，并不得与 Foundation 内置连接器重名。
- Delta Suite 的安装器 `advanced/install.py` 只写入两个不含密钥的清单到 `<state>/extensions/delta-suite/`，`pyproject.toml` 固定 Foundation 的不可变提交（`git-ref`）以及 `capability-abi = 2`、`worker-manifest = 1`、`connector-catalog = 1`。
- Suite 的 Python 包对完整 `delta` 发行版有依赖，因为 `delta_extension_api` 仍委托给 Foundation 的 `integrations.*` 与 `packages.*` 模块。这是 Suite 文档已经记录的迁移债务，不是加载机制的问题。

## 3. 备选方案

### A. Python entry-point 组 `delta.extensions`

- 优点：符合 Python 打包习惯，安装包即生效，不需要额外安装步骤。
- 缺点：
  - 加载发生在 Python 进程内，扩展代码随发现过程被导入，与"Foundation 绝不导入可选扩展源码"的规则相冲突；
  - 一个坏扩展可能在导入时阻止启动，而清单方式可以逐个隔离；
  - 无法在不导入代码的前提下诊断扩展，也无法只删除扩展的激活配置；
  - 绕过 Rust 拥有的授权、监督与 Ledger 注册路径，形成第二套激活入口；
  - 与 Suite 已有的 CI 规则和 `ARCHITECTURE_BOUNDARY.md` 直接冲突。

### B. 已安装清单（选定）

- 优点：
  - 已经在 Rust 与 Suite 中实现并通过 Suite 的 CI 检查；
  - 清单在执行前由 Foundation 校验，格式错误只影响该扩展；
  - 扩展代码不在 Foundation 进程中导入；授权、进程监督与 Ledger 仍由 Rust 拥有；
  - 删除 `<state>/extensions/<source>/` 即可移除扩展，诊断只需读取清单；
  - 清单是数据，Suite 只固定格式版本与 Foundation 提交，不需要共享 Python 内部实现。
- 缺点：
  - 需要安装步骤和重启 Runtime 才能生效；
  - 清单中的 `program` 是可执行路径，信任边界是本机用户，而非签名；
  - 清单格式目前没有 `schemas/` 下的 JSON Schema（见第 5 节）；
  - 运行时不校验扩展针对的 Capability ABI 或合同版本，兼容性只在安装与构建时通过固定提交保证。

### C. 两者并用（entry point 发现，清单激活）

不选。它产生两个来源的真相，违背 Foundation 作为唯一权威平面的要求。

## 4. 决定

1. 可选扩展只通过 `<state>/extensions/<source>/` 下的已安装清单激活（方案 B）。
2. 扩展只能使用公开合同：`delta-sdk`（Rust）、`delta_worker_sdk` 与 `delta_extension_api`（Python）以及清单格式。不得导入 Foundation 内部模块，不得依赖 Foundation 的 main 分支。
3. 兼容性以固定版本表达：Suite 固定 Foundation 的不可变提交与清单格式版本；Foundation 拒绝版本不符的清单。
4. Python entry points 不是激活机制，也不得在仓库中注册（由 `scripts/check_architecture_boundary.py` 的 `entry-point-discovery` 规则强制执行）。

本次为落实决定所做的最小修改：

- `contracts/CONTRACT_RULES.md` 稳定性规则 3 改为引用清单目录，并说明 entry point 不是激活机制；
- `scripts/check_architecture_boundary.py`：把 `entry-point-consistency` 规则（要求组名为 `delta.extensions`）替换为 `entry-point-discovery`（禁止 entry-point 注册）；当前仓库没有任何匹配，检查通过；
- `docs/architecture/repository-layout.md` 中的过时说明改为指向本文。

Rust 加载器与 Suite 的清单写入代码没有需要改正的错误，因此没有修改代码。Suite 仓库已经与该决定一致，本次不需要修改。

## 5. 决定之后仍未解决的问题

以下问题需要单独的合同变更或所有者决定，本次没有实施：

1. **清单缺少 schema。** `CONTRACT_GOVERNANCE` 规则 4 要求跨进程数据结构先在 `schemas/` 中有 JSON Schema。已安装的 `capability-workers.json` 与 `connector-catalog.json` 目前没有。
2. **两种 "worker manifest" 形状并存。** `schemas/capability-manifest.schema.json` 与 `delta-sdk` 的 `WorkerManifest` 使用 `worker_id` + `capabilities`；Rust 加载器读取的已安装清单使用 `version` + `source` + `workers`。哪一种是权威格式，需要所有者决定，然后再补 schema。
3. **`delta_extension_api` 仍依赖 Foundation 内部。** 该公开门面直接导入 `integrations.*` 与 `packages.*`，与第 4 节第 2 条矛盾。消除依赖需要把实现抽离为公开接口面，属于较大的迁移，不是最小修改。
4. **清单不声明目标 ABI 或合同版本。** Foundation 运行时无法拒绝针对错误 ABI 编写的清单。增加必填字段是合同变更，需要评审并提升版本。

## 6. 影响

- Delta Foundation：无运行时行为变化；只改变文档、合同 README 与架构检查规则。
- Delta Suite：安装器与清单格式不变；其文档与 CI 已与本决定一致。
- 扩展作者：只能依赖公开 SDK 与清单格式，不得以 entry point 注册扩展。
