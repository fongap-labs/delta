# Delta

Delta 是一个本地优先、可插拔的 Agent 平台：既能独立运行，也能作为外部 Harness 的插件运行。

## 核心结构

- **Desktop**：统一体验层（React/TypeScript）；沿用现有 Tauri 2 实现，Electron 作为可替换候选。
- **Core**：Rust 稳定内核，负责会话状态、权限、审批、插件管理及持久化边界。
- **Runtime**：两种执行模式——Delta Runtime（原生）与 Harness Runtime（宿主）。
- **Adapter**：按 Harness 分别实现适配，首个目标是 DeepSeek Harness；不污染 Core。
- **Worker**：跨运行模式复用，受 Core 的授权、隔离与审计约束。

## 协议方向

基础进程通信优先 JSON-RPC 2.0；客户端交互语义评估 ACP；工具扩展采用 MCP；A2A 按真实协作需求接入。宿主端坚持其原生协议，由 Adapter 转换。具体接口以版本化合同和原型验证为准。

## 开发路线

1. 在本仓库建立干净的架构与接口基线。
2. 审计并择优迁移 `delta-legacy` 的 Core、Desktop、SDK 和品牌资产。
3. 从 `dsh-plugin` 提取 DSH Adapter 与通用 Worker，先验证宿主模式。
4. 补齐原生 Agent Engine，再验证独立模式；两种模式共用核心接口。

**当前状态：架构及治理起草阶段；功能和兼容性未宣称已实现。**

文档入口：[架构](docs/ARCHITECTURE.md) · [迁移](docs/MIGRATION_PLAN.md) · [品牌](docs/BRAND.md) · [项目治理](docs/GOVERNANCE.md) · [贡献](CONTRIBUTING.md)。

跨仓库合并、命名、发布与安全治理统一以 [Action Worker](https://github.com/fongap-labs/action-worker/tree/main/docs) 为权威。
