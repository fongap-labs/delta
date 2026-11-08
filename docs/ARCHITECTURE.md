# Delta 目标架构（草案）

## 产品定位

Delta 是本地优先、可独立运行且可作为 Harness 插件运行的模块化 Agent 平台，不是 DeepSeek Harness 的 UI 分支。

## 固定的组件边界

| 组件 | 权威职责 | 技术方向 |
| --- | --- | --- |
| Desktop | 界面、会话展示、进度、审批交互 | React/TypeScript；现有 Tauri 2 优先复用，Electron 待同条件验证 |
| Core | 权限、审批、信任、会话状态、插件宿主、持久化 | Rust；不依赖任何桌面框架和具体 Harness |
| Runtime | 单个会话的 Agent 执行循环与调度 | Delta Runtime 或 Harness Runtime（二选一） |
| Adapter | Harness 原生能力映射、协议/生命周期适配 | 按宿主分别实现；DSH Adapter 为首个目标 |
| Worker | 受控任务执行、工具与专用能力 | 共用协议与能力合同，按需隔离进程 |

## 两种运行模式

**Delta Runtime（原生）**：Delta 自有 Agent Engine、模型与工具调度；不依赖 DSH。

**Harness Runtime（宿主）**：复用宿主 Agent Engine；Adapter 将宿主会话、审批、运行事件及工具映射到 Delta；必须保留 DSH 等宿主的原生插件能力。

运行模式是一次会话的执行选择，不对应两套独立 Desktop 或 Core。一个会话只允许一个 Agent 执行循环作为调度权威，Core 仍统一实施自身的权限与交互政策。

## 协议

- 内部进程消息以 JSON-RPC 2.0 为基础；本地优先 stdio / Unix Socket / Windows Named Pipe 等适合的 IPC。
- Desktop ↔ Core 面向 ACP 会话/事件语义设计可映射接口；ACP 完整兼容需单独验证。
- Core ↔ Adapter 使用版本化能力合同；Adapter ↔ Harness 遵循宿主公开支持的原生协议。
- 工具与资源优先 MCP；Agent 间协作有真实需求时才添加 A2A。
- 所有跨进程调用明确版本、能力协商、取消、背压、错误、幂等与授权上下文。

## UI 接管与回退

宿主模式下目标为优先启动 Delta Desktop，只有确认界面及会话通道健康后才关闭/隐藏宿主原生 UI。若 Desktop 启动失败则恢复原生 UI；禁止出现两套 Agent 循环或重复执行副作用。该目标依赖 DSH 插件生命周期验证，目前不宣称已经实现。

## 安全及插件约束

插件、Worker 与 Harness 只获得明确授予的能力；关键审批和高后果操作不得由 UI、Agent、插件自行提升权限。Core 不直接加载不可控第三方代码到同一安全域。兼容性以实际宿主版本矩阵和合同测试为准。

