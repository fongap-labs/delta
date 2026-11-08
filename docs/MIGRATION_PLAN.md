# Delta 迁移计划（草案）

## 来源与原则

- 新主仓库：[`fongap-labs/delta`](https://github.com/fongap-labs/delta)，存放新结构的权威代码。
- 旧代码资产：[`delta-legacy`](https://github.com/fongap-labs/delta-legacy)，优先评估 Desktop、Rust Core、SDK、测试与品牌。
- 宿主插件资产：[`dsh-plugin`](https://github.com/fongap-labs/dsh-plugin)，优先评估 DSH Desktop 接入和 Delta Code Worker。
- 扩展资产：[`delta-suite`](https://github.com/fongap-labs/delta-suite)，按公共合同择优迁移能力。

不推倒重写，也不整仓复制。迁移单元是可单独验证的模块；原仓库保留作为来源与回退依据。

## 迁移顺序

1. **治理和合同**：冻结组件权威、统一 IPC 消息模型和运行时能力合同，明确平台验证矩阵。
2. **Core 与 SDK**：审计旧 Rust 模块，剔除 Tauri、旧界面、DSH 特有依赖；建立合同/安全测试。
3. **Desktop 和品牌**：复用 React 组件及设计 token，以 Tauri 2 跑通最小窗口、连接、会话和审批；同条件评估 Electron。
4. **DSH Adapter**：从插件工程提取；验证启动、握手、流式消息、工具调用、原生插件、关闭原生 UI 与故障回退。
5. **Worker**：以权限受控的独立能力接口运行；验证两种 Runtime 的一致行为与失败隔离。
6. **Delta Runtime**：接入自主 Agent Engine 和模型调度，完成独立运行闭环。
7. **迁移收口**：双模式回归、平台打包、来源与许可证核验、性能基准、发布资格验证。

## 每个模块的验收

需提交来源文件与版本、依赖差异、权威边界、合同测试、失败路径、性能影响及可回退方案；不能把旧代码直接存在当作已验证。

第一批产品源码进入本仓库时，必须在同一 PR 中将引导 CI 升级为真实 Rust/TypeScript 等项目检查，遵守中央 Action Worker 门禁。

