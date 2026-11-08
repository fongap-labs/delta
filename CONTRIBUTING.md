# 参与开发

本仓库只约束 Delta 的产品、架构、合同与验证，不另建组织级贡献规则。

## 共同治理

提交、PR、CHANGELOG、中央门禁、主分支来源证明及发布规则，遵循 [Action Worker 共同治理](https://github.com/fongap-labs/action-worker/blob/main/docs/01_SHARED_GOVERNANCE.md) 与 [贡献指南](https://github.com/fongap-labs/action-worker/blob/main/CONTRIBUTING.md)。

如有冲突，按「中央机器合同/策略 → 中央共用治理 → Delta 项目边界 → 实施说明」裁决。任何 Agent 不得自行修改或放宽中央门禁；需要维护者就具体变更明确授权。

## Delta 专有要求

1. 单个变更聚焦一个可验证目标；接口变更同步更新合同、消费者测试与迁移说明。
2. Core 不导入 Tauri、Electron 或某个 Harness 的专用实现；UI 不得越过 Core 审批。
3. 不同 Harness 的特殊行为归各自 Adapter；Worker 不持有会话、审批和核心数据库的最终权威。
4. Harness 模式必须保留宿主原生插件生态；Delta 原生模式不承担该兼容承诺。
5. 涉及 UI 接管、启动失败回退、会话恢复或高风险操作，必须验证失败路径。
6. 迁移旧代码需保留来源、许可证、资产清单及回归测试，不能把旧实现的全部设计决定自动继承为新架构。

## 验证与提交

新增产品源码的同一个 PR 必须建立对应语言、平台和模块的真实构建及测试，不允许空测试或绕过 `.github/execution-manifest.json` 的中央 CI。

PR 中说明：变更目标、影响的运行模式、关键风险、验证命令与结果、是否涉及协议兼容或用户数据迁移。

通用命名、提交类别及变更记录格式仅引用中央权威：[命名](https://github.com/fongap-labs/action-worker/blob/main/docs/03_NAMING_CONVENTIONS.md) · [变更](https://github.com/fongap-labs/action-worker/blob/main/docs/04_CHANGELOG_CONVENTIONS.md)。

