# ADR-0082：Foundation 与 Suite 的干净切割

状态：**已接受**（所有者，2026-10-27，要求切断两个仓库的实现耦合，防止后期回弹）。

## 背景

- Delta Foundation 与 Delta Suite 的边界文档一直规定“只通过公开合同接入”，但 Suite 当前仍在 `pyproject.toml` 中依赖完整的 `delta` 发行版，并在浏览器与 Connector 代码中导入 `delta_extension_api`。
- `delta_extension_api` 是一个门面：它直接导入 Foundation 的 `integrations.*` 与 `packages.*` 内部模块。因此 Suite 实际上仍然依赖 Foundation 内部实现，只是多了一层门面。
- Suite 还固定 Foundation 的不可变提交；Foundation 的任何内部变化都可能迫使 Suite 跟版。这是“反跳”的来源：一旦为了修复 Suite 而再次引用 Foundation 内部模块，边界就会重新粘连。
- CMC 的 R 代码已经迁入 Suite，新的 CMC Worker 只依赖 `delta-worker-sdk` 与 `delta.result` 合同；这说明干净切割在新增代码上是可行的。

## 决定

### 1. 唯一允许的依赖面

Suite 只允许依赖一个**独立的合同包**（暂名 `delta-contracts`）：

- Worker ABI 的 job / result / artifact / progress 模型；
- Capability 与 Worker Manifest 的 Schema 与类型；
- Connector 描述符、工具元数据与目录 Manifest 的合同类型；
- `SecretSource` 等只读协议；
- 浏览器连接地址等仅通过环境 / 参数传递的合同值；
- JSON Schema 与合同测试 fixture。

合同包不得导入 Foundation 的 `integrations.*`、`packages.*`、`core.*` 或任何 Runtime / Trust / Ledger 内部模块。

### 2. Foundation 的约束

- Foundation 源码不得命名或导入 Suite 的实现标识（`delta-suite`、`delta_suite`、Suite capability ID）。文档可以提及 Suite，但代码不得有分支。
- Foundation 只依赖合同包做 Manifest / ABI 校验，不依赖 Suite 的 Python 包、R 包或安装目录。
- Foundation 的测试与构建不要求安装 Suite。

### 3. Suite 的约束

- Suite 不得依赖完整的 `delta` 发行版，不得导入 `delta_extension_api`、`integrations.*`、`packages.*`、`core.*` 或 `projects/delta/crates/delta-core`。
- Suite 不再固定 Foundation 的提交；只固定合同包的语义版本（以及 Capability ABI / Manifest 版本）。
- Suite 的浏览器、Connector、CMC 与报告模板都在 Suite 内实现，不通过 Foundation 内部模块“借用”实现。
- Suite 的测试在未安装 `delta` 的环境中可以运行。

### 4. 共享资产

- R 4.6.0 与 R 包 pin 是数据资产，可以放在 Suite；它们不是 Foundation 代码。
- 报告结果合同、方法包合同与绑定清单合同通过 JSON Schema 分发，不通过复制 Foundation 源码分发。

### 5. CI 闸门

- Suite：在 `delta` 未安装的环境中运行测试；禁止新的 `delta_extension_api` / `delta` 导入；禁止 `pyproject.toml` 依赖完整 `delta`；合同包缺失时失败。
- Foundation：禁止源码中出现 Suite 实现标识；禁止依赖 Suite 包；在未安装 Suite 的环境中构建与测试。
- 两边：合同 fixture 与 Schema 兼容性测试；破坏性合同变更需要版本升级。

### 6. 迁移阶段

1. **冻结新增耦合。** 新代码（包括 CMC）只依赖 `delta-worker-sdk` 与结果合同；加入“不得新增 `delta_extension_api` 导入”的棘轮。
2. **抽合同包。** 把 Worker 协议、Manifest Schema、Connector 描述符、SecretSource 与浏览器连接约定抽成独立包；`delta_extension_api` 只保留薄兼容层或标记弃用。
3. **迁移 Suite。** 浏览器、Connector 与测试改为只依赖合同包；移除完整 `delta` 依赖与 Foundation `git-ref`；Suite 在无 Foundation 的环境跑通测试。
4. **收尾。** Foundation 删除或冻结 `delta_extension_api`；Suite 独立发布；两边只共享合同版本。

## 后果

- Suite 安装体积与启动依赖下降，不再被 Foundation 内部重构拖着走。
- 合同包成为新的稳定边界，需要自己的版本、测试与发布节奏。
- 浏览器连接与 Connector 工具的部分实现必须从 Foundation 迁到 Suite 或合同包，属于有意的代码移动。
- 迁移完成前，`delta_extension_api` 仍是已知债务；用棘轮阻止新增使用，而不是一次硬切。

## 实现记录（2026-10-27）

- 曾建立只含 Manifest 模型的 `delta_contracts` 原型，随后删除：以当前两个第一方仓库的规模，Suite 只需要公开 `delta-worker-sdk` 与 JSON Manifest，不需要额外的 Python 合同包。它不进入任何运行时路径。
- Suite 已移除完整 `delta` 发行版与 `delta_extension_api`。浏览器、Connector、SecretStore、HTTP 地址守卫、邮箱校验与工具元数据改由 Suite 自己的 `advanced.extension_api` 提供。
- Suite 现在只依赖 `delta-worker-sdk`、`httpx` 与 `playwright`；测试在未安装完整 `delta` 的环境中通过（44 passed, 2 skipped）。
- `projects/delta/scripts/check_boundaries.py` 的 `CLEAN_CUT_DEBT` 为空；新的 Foundation 门面导入或完整 `delta` 依赖会失败。
- Worker ABI 模型仍留在 `projects/delta/packages/delta_worker_sdk`；后续如果独立发布 SDK，再把 `foundation.git-ref` 换成 SDK 版本号。

## 未验证事项与未决事项

- 合同包的名称、仓库位置与发布方式尚未确定。
- 浏览器连接地址的合同形状需要与 Suite 的浏览器 Worker 对齐。
- Connector 描述符与 HTTP 工具执行的所有权需要逐项决定：合同类型放合同包，执行实现放 Suite。
- Suite 当前仍有 6 个文件导入 `delta_extension_api`；迁移完成前它们是明确的债务清单。
- 现有 `delta_extension_api` 的公开导入路径需要一段弃用期，避免第三方扩展立即断裂。
