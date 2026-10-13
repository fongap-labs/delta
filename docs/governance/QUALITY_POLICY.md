# 质量治理

本文规定 Delta 的持续集成、测试、质量门禁和安全检查要求。

质量目标统一服务于：

```text
长期 | 稳定 | 高效 | 安全 | 敏捷
```

## 基本原则

Delta 的质量控制应尽量通过自动化验证完成。

人工 Review 用于判断产品边界、模块职责、Authority、安全和风险，不替代 CI。

任何变更都不应通过降低既有质量标准换取合并。

## 必需检查（Required Check）

`validate-merge` 是 `main` 的稳定 Required Check。

内部 CI 作业可以演进，但应保持 `validate-merge` 名称稳定，避免 Ruleset 因内部作业重命名而失效。

新增、删除或重命名 CI 作业时，必须同步检查 `validate-merge` 的依赖关系。

## CI 范围

根据变更范围，CI 应覆盖：

- Python / Worker 测试；
- TypeScript 测试；
- 前端 e2e；
- Rust 构建 / 测试；
- Rust / TypeScript 合同测试；
- Ruff / Pyright（Python 仍存在期间）；
- format / Clippy；
- 依赖与安全扫描；
- 仓库布局（repository layout）；
- 打包验证。

涉及核心 Runtime、Trust、Work、Capability ABI、构建或打包的变更不得跳过关键验证。

## Runtime 收敛门禁（Runtime Convergence Gates）

CI 应持续强制：

- Rust Runtime 合同测试；
- TypeScript ↔ Tauri / Rust IPC 合同；
- Steer / Follow-up / Cancel e2e；
- Capability ABI 兼容性；
- Worker 崩溃 / 超时 / 取消 / 格式错误的响应；
- 禁止 Python 核心权威回归；
- 禁止 Worker 直接写核心数据库；
- Rust Authority 域无 Python 回退写入方；
- 无新增持久化 Python 应用权威；
- portable / 发布冒烟测试（release smoke）。

架构验收关注**行为和 Authority**，不关注 Rust 百分比或 Python 文件数。

## 研究分析质量（Research Analysis Quality）

研究分析不能只验证“代码执行成功”。

涉及统计、DOE、序贯试验和正式研究报告时，根据能力范围应增加：

- 数据身份 / hash；
- 数值结果回归 / 容差；
- 统计方法前提；
- 随机种子 / 可复现性（适用时）；
- 设计矩阵可估计性；
- DOE / 序贯设计约束保持；
- 停止规则和允许调整范围不得被静默改变；
- 图表 / 结构化结果 / 报告 Artifact 的完整性；
- 来源与引用一致性（Source / Citation consistency）。

模型的自然语言解释不能替代可确定验证的统计计算和设计约束。

## 内容 / 媒体质量（Content / Media Quality）

图文、图片、视频 Capability 根据任务需要验证：

- 文件格式与可打开性；
- 尺寸 / 时长 / codec / media type；
- 输出文件完整性；
- 平台规格；
- 来源 / 权利元数据（适用时）；
- 资源消耗和超时；
- Worker 暂存边界（staging boundary）。

不要求核心 Runtime 判断审美优劣，但必须判断成果是否真实存在、格式可用、没有越权输出。

## Skill / 学习质量（Skill / Learning Quality）

Skill 候选（Skill Candidate）或学习（Learning）结果进入长期使用前，必须有可观察的证据。

至少根据风险使用：

- 历史 Run 重放（replay）；
- Validation；
- 成功 / 失败率；
- 用户采纳 / 修改；
- 回归任务；
- 版本比较。

Learning 不得通过测试或配置自动降低 Policy / Approval / Risk / Sandbox / Network / Secrets 边界。

## layout-check（仓库布局检查）

`layout-check` 保护当前仓库的物理结构和路径一致性。

至少检查：

- 非法顶层目录；
- 已废弃路径重新出现；
- 关键目录职责边界；
- 构建 / 发布路径残留；
- 已删除的 Python Backend 路径是否被恢复。

目标架构的七个逻辑模块不要求创建七个同名目录；不得为了“符合架构图”提前修改 layout-check。

## 测试原则

新增功能应覆盖核心行为；缺陷修复应尽量增加回归测试；重构应优先证明行为保持。

不得：

- 删除正常测试来掩盖失败；
- 无依据增加 `skip`；
- 扩大 ignore；
- 降低断言；
- 删除关键测试路径；
- 只用手工验证替代已有自动测试。

测试应优先覆盖稳定的 Runtime / Capability / IPC 合同，而不是绑定内部 class 或 endpoint 实现。

## CI 失败分类

出现失败时，先判断：

1. 本次变更引入；
2. 已存在问题；
3. 环境问题；
4. 外部服务问题；
5. 依赖 / 工具链变化。

认定为既存或外部问题时应有证据。“本地可以运行”不能绕过 CI。

## 安全检查

代码、文档、配置和测试不得提交真实的：

- API Key / Access Token；
- Password / Private Key / PFX；
- 用户私有数据；
- 内部服务凭据。

示例必须使用明显的占位值。

Worker / Skill / Connector / Learning 还必须验证其不能绕过 Trust 或获取未声明的资源。

## 依赖安全

依赖变更检查：

- 已知漏洞；
- License；
- Breaking change；
- Runtime / 数值行为；
- 构建 / 打包；
- Worker sandbox / 二进制再分发。

详见 [DEPENDENCY_POLICY.md](DEPENDENCY_POLICY.md)。

## 合并要求

进入 `main` 前至少满足：

1. `validate-merge` 通过；
2. 关键合同 / 回归测试通过；
3. 无已知未处理的安全问题；
4. 没有通过放宽检查掩盖失败；
5. 相关文档已同步；
6. 变更范围与 Pull Request 一致；
7. Authority 前后对比（Before / After）与代码一致（涉及迁移时）；
8. 新 Capability / Skill 未绕过 Trust / Work / Validation；
9. 三个产品域的真实任务没有因架构迁移而无证据地退化。

无法满足时，必须明确原因和影响；临时例外不能自动变成长期规则。
