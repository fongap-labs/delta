# Delta 项目治理

## 唯一权威

跨仓库治理的唯一权威是 [fongap-labs/action-worker](https://github.com/fongap-labs/action-worker/tree/main/docs) 的机器合同、策略和共用规范；本仓库只定义 Delta 专属的产品边界、对外协议、迁移与质量要求。

禁止在 Delta 复制命名、提交类型、中央 CI、合并、主分支保护或发布策略，也禁止通过项目文件降低其约束。

## 项目权威

- 产品与组件职责：[ARCHITECTURE.md](ARCHITECTURE.md)。
- 代码资产与迁移准入：[MIGRATION_PLAN.md](MIGRATION_PLAN.md)。
- Logo、颜色和生成资产来源：[BRAND.md](BRAND.md)。
- 参与开发的项目特有要求：[CONTRIBUTING.md](../CONTRIBUTING.md)。

## 执行边界

变更涉及 Runtime 权威、信任边界、IPC 或兼容性协议时，先给出影响分析和可执行测试，验证消费者后再合并。禁止 Adapter 反向侵入 Core；禁止 Worker 绕过权限/审批。已有代码不因迁移而自动获得可信或发布资格。

## 组织治理接入

中央受管仓库的真实约束是 `.github/execution-manifest.json` 与 Action Worker 下发的校验结果；仓库只保留薄入口和不可外置的项目测试。新产品源码必须同时升级真实 CI，不能靠文档校验代替。

相关中央权威：[共用治理](https://github.com/fongap-labs/action-worker/blob/main/docs/01_SHARED_GOVERNANCE.md) · [仓库治理](https://github.com/fongap-labs/action-worker/blob/main/docs/02_REPOSITORY_GOVERNANCE.md) · [接入规范](https://github.com/fongap-labs/action-worker/blob/main/docs/05_INTEGRATION_GUIDE.md) · [主分支证明](https://github.com/fongap-labs/action-worker/blob/main/docs/07_MAIN_WRITE_GUARD.md)。

