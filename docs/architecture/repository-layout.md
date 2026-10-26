# 仓库目录规范

本文说明 Delta 仓库如何按系统职责组织代码，以及目录与 crate 的命名规则。

> 本文只描述**当前有效的物理仓库结构与命名规则**。长期逻辑模块和目标依赖方向见 [`target-architecture.md`](target-architecture.md)。

Experience / Runtime / Trust / Work / Capability / Automation / Learning 是逻辑职责，不要求机械地创建同名的顶层目录、crate 或进程。

## 命名总原则

> **独立边界用 `delta-*`，内部职责用语义名；品牌化用于识别产品边界，不用于包装每一个内部模块。**

`delta-*` 表示"这是 Delta 的一个独立边界"；不带品牌前缀的语义名表示"这是边界内部的职责"。

### 可以使用 `delta-*` 的情况

只有形成明确的独立边界时才使用，例如满足至少一项：

- 独立可执行程序；
- 独立 crate / package，且存在明确的公共 API；
- 独立的版本兼容边界；
- 独立的跨语言协议；
- 独立 SDK；
- 独立的发布 / 分发单元。

推荐命名：

| 层级 | 建议名称 | 用途 |
| --- | --- | --- |
| 桌面主程序 | `Delta` / `Delta.exe` | 用户直接运行 |
| Rust 核心 crate | `delta-runtime` | 核心 Runtime 独立边界（当前实际 crate 为 `delta-core`） |
| IPC / 公共协议 | `delta-protocol` | TS ↔ Rust 的合同 / 事件 / DTO |
| Capability SDK / ABI | `delta-capability` | Capability 合同 / 清单 / 请求 / 结果 |
| Worker 启动器 | `delta-worker` | Rust 管理的 Worker runner |
| Python Worker SDK | `delta-worker-py` | 仅在形成独立 Python SDK 时存在（当前实际包为 `delta-worker-sdk`，位于 `packages/delta_worker_sdk`） |
| CLI | `delta-cli` | 仅在保留独立 CLI 产品边界时存在 |
| 测试工具 | `delta-testkit` | 仅在形成稳定的公共测试工具时存在 |
| TS workspace package | `@delta/*` | 仅在真正拆成独立 package 时使用 |

上表是命名原则下的推荐名称。标注为"当前实际"的之外，表中其余名称尚未在仓库中实现，不代表当前结构。

### 内部职责不品牌化

以下内部职责保持语义名：

```text
runtime/
  session/
  run/
  context/
  provider/
  tool/

trust/
  policy/
  approval/
  ledger/
  secrets/

work/
  source/
  citation/
  artifact/
  validation/

learning/
  memory/
  experience/
  skill/
```

除非未来真实形成独立的发布、版本、协议或进程边界，否则不要创建：

```text
delta-trust
delta-work
delta-learning
delta-approval
delta-artifact
```

## 当前顶层目录

| 目录 | 当前职责 |
| --- | --- |
| `apps/` | 用户可运行的应用（当前为 `apps/desktop/`） |
| `contracts/` | 版本化的跨进程 / 跨仓库合同定义（`README.md` 为中文说明，`CONTRACT_RULES.md` 为英文机器读取规则；JSON Schema 位于 `schemas/`） |
| `core/` | 少量 Python 辅助代码（`roots.py`、`pdf_support.py`），不再承载 Runtime |
| `crates/` | Rust workspace crates（`delta-core`、`delta-computer`、`delta-stt`、`delta-sdk` 等） |
| `delta_extension_api/` | 面向已安装扩展的稳定 Python 扩展 API |
| `integrations/` | Connector、Tool、Web 等外部能力（当前子目录：`capabilities`、`connectors`、`managed`、`tools`、`web`） |
| `packages/` | 跨模块的基础能力、迁移 bridge，以及 Python Worker SDK（`delta_worker_sdk`） |
| `resources/` | 品牌、截图等非代码资源 |
| `packaging/` | 构建、打包、发布（当前为便携版 `packaging/portable/`） |
| `schemas/` | JSON Schema 合同数据结构定义 |
| `scripts/` | 仓库维护与验证脚本 |
| `docs/` | 产品、架构、治理与运维文档 |
| `.github/` | GitHub 配置与自动化 |
| （无 `tests/`） | Python 合同与兼容测试位于 `fongap-labs/action-worker` 的 `tests/packs/delta/`；Rust 与桌面端测试仍与代码同目录 |

## `apps/desktop/`

当前桌面端已通过 Tauri Commands / Events 直接连接进程内的 Rust `RuntimeHost`。PR #203 已移除 Tauri 主路径对 Python `delta-server` sidecar、localhost 反向代理以及随包 sidecar 资源的依赖。

浏览器开发和部分兼容路径仍可能使用 Python server；但它已不再是桌面端的主产品路径。

TypeScript 只负责 Experience / UI，不拥有业务权威。

## `core/`

`core/` 当前仅含少量 Python 辅助代码（`roots.py`、`pdf_support.py`）。原先的 Python Runtime / Agent glue（ledger、analyzer、automation 等）已不在此目录中；Rust Runtime 实现位于 `crates/delta-core/`。

迁移期间的约束：

- 新的控制面默认进入 Rust；
- Python `TurnEngine` / application glue 属于迁移对象；
- Python Office / Research / Media 专业能力优先 Worker 化；
- 调用方完成迁移后删除 facade，不长期保留纯转发链。

## `crates/`

Rust workspace crates，每个 crate 对应一个独立的 Rust 边界：

| Crate | 职责 |
| --- | --- |
| `delta-core` | Rust Runtime、可信权威与控制面（原 `core/runtime-native`） |
| `delta-stt` | 独立的 STT 能力（原 `services/stt`） |
| `delta-computer` | 桌面电脑能力：屏幕查看、剪贴板、打开文件与链接（ADR-0059），以及受控浏览器（ADR-0060）。平台代码放在这里，不进入 `delta-core` |
| `delta-sdk` | Foundation SDK：能力、Worker、连接器、产出物、事件合同 |
| `delta-connect` | 连接器注册表合同 |
| `delta-sync` | 中继 / OAuth broker 合同及 Null 实现 |

## `contracts/`

版本化的跨进程 / 跨仓库合同定义。合同是 Foundation 与 Suite 之间唯一的扩展边界。合同变更需要版本升级和合同测试。

## 模型边界

模型边界固定为：

```text
OpenAI-compatible
Anthropic-compatible
```

仓库中没有独立的 `providers/` 目录；该边界的实现位于 `crates/delta-core`（`provider.rs`、`model_authority.rs`、`model_catalog.rs`）。

迁移目标是删除不再需要的 Python Provider 控制与 SDK 回退，而不是扩展更多 Provider 系列。复杂的多 Provider 调度外置给兼容端点 / AI 网关。

## `integrations/`

长期属于 Capability / Automation 边界。

约束：

- Integration 不拥有 Runtime / Trust / Work 权威；
- Skill 不直接获得额外权限；
- MCP / Connector 不绕过 Policy / Approval / Ledger / Validation；
- Python 能力通过受控的 Capability ABI 使用；
- Connector / provider 代码只依赖只读的 `SecretSource`，不得自行创建文件型 `CredentialStore`；具体的保险库只允许出现在 headless / operator 兼容入口中；
- Connector 工具的启用状态由 Rust `ApplicationStore` 统一拥有，不得写入 `secrets.json`，也不得由 Extension 维护第二套状态。

## `delta_extension_api/`

面向已安装扩展的稳定 Python 扩展 API。扩展应从此包导入，而不是从 Foundation 的内部模块 `integrations.*` 或 `packages.*` 导入。

仓库中不存在 `delta.extensions` entry-point 加载代码，也不得新增：可选扩展只通过 `<state>/extensions/<source>/` 下的已安装清单激活，决定与理由见 [extension-loading.md](extension-loading.md)。架构边界检查（`scripts/check_architecture_boundary.py`）禁止 entry-point 注册。

## `schemas/`

JSON Schema 定义，用于跨进程合同数据结构的版本化验证。当前包括能力清单（`capability-manifest.schema.json`）、执行授权（`execution-grant.schema.json`）、多轮研究记录（`study.schema.json`，ADR-0056）、分析结果、分析环境、方法包与绑定清单（ADR-0079、ADR-0080），以及产品版本标记（`product-edition.schema.json`，ADR-0083）。

## `packages/`

只保存真正跨模块共享、职责明确的基础能力与迁移 bridge。不得成为 `common/` / `utils/` 式的收容目录。

Python Worker 的唯一权威 SDK 是 `packages/delta_worker_sdk`（参见 [`capability-abi.md`](capability-abi.md) 第 19 节）。

当前共享面是 `delta_worker_sdk`、JSON Manifest 与 `delta.result` 合同。以两个第一方仓库的规模，不单独维护 Python 合同包（ADR-0082）；若未来出现第三方扩展生态，再评估是否需要。

## `docs/`

```text
docs/
├── DELTA_BLUEPRINT.md
├── architecture/
├── governance/
└── operations/
```

权威入口：

- 产品：[`docs/DELTA_BLUEPRINT.md`](../DELTA_BLUEPRINT.md)；
- 目标架构：[`docs/architecture/target-architecture.md`](target-architecture.md)；
- 当前 Runtime 合同：[`docs/architecture/runtime-public-contract.md`](runtime-public-contract.md)；
- Capability ABI：[`docs/architecture/capability-abi.md`](capability-abi.md)；
- 当前结构与命名：本文（[`docs/architecture/repository-layout.md`](repository-layout.md)）；
- 历史决策：[`docs/architecture/adr/`](adr/README.md)；
- 治理文档：[`docs/governance/`](../governance/)。

已完成且不再定义当前架构的一次性迁移说明，不留在 active 的架构目录中；历史通过 Git / PR / ADR / CHANGELOG 追溯。

## 目标依赖方向

```text
Experience (TypeScript)
        ↓
Runtime (Rust)
   ┌────┼────┐
   ▼    ▼    ▼
 Trust Work Capability
   │           │
   ├──────┐    ▼
   ▼      ▼  Worker / External
Automation Learning
```

## 禁止模糊目录

原则上不得新增：

```text
common/
misc/
helpers/
utils/
shared/
base/
legacy/
```

如果代码无法归类，先检查其职责与 Authority，而不是先建新的容器。

## 目录 / 命名变更

以下视为架构治理变更：

- 新增 / 删除顶层目录；
- 新增独立的 `delta-*` crate / package / 可执行程序；
- 模块职责迁移；
- 关键依赖方向变化；
- Python Backend 物理退出；
- 新增长期 Worker 根目录；
- 大规模路径调整。

必须说明：为什么它是独立边界、Authority 属于谁、兼容与迁移方式、CI / 文档如何同步。

## 当前结构优先

本文只记录当前有效的物理事实与命名规则。目标结构不能通过提前建空目录或大量 `delta-*` 包装来伪装成已实现。
