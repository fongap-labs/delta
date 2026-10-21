# 参与 Delta 开发

本文只列出贡献者需要立即了解的规则；详细治理见 [docs/governance/](docs/governance/README.md)。

## 产品边界

Delta 的一级产品方向固定为：

```text
日常办公
研究分析
内容创作
```

研究分析包含统计分析、DOE（实验设计）、**序贯试验设计**、模型更新、下一轮试验建议、停止判断和正式研究报告。

内容创作包含文本、图文、图片、视频、多平台适配、发布准备和内容复盘。

新增能力必须明确回答：它改善三项核心能力中的哪个真实工作环节？否则不进入产品主线。

## Agent / Capability / Skill

Delta 保持一个统一的主 Agent。三个产品域不对应三个独立的 Agent 或 Runtime。

优先级：

```text
现有 Capability 组合
→ Skill
→ 新 Capability / Worker
→ 扩展 Runtime（仅确有必要）
```

PDF / Office、Search、Citation、Validation、Scripting、Automation、Connector、MCP、图像 / 视频模型等属于能力（Capability），不提升为新的一级产品方向。

## 架构边界

长期逻辑职责：

```text
Experience
Runtime
Trust
Work
Capability
Automation
Learning
```

核心产品语言目标：

```text
Rust + TypeScript
```

Python / PowerShell / Shell 是受控 Worker / Script，不拥有 Session、Run、Policy / Approval、core DB、Artifact 正式状态（formal state）、Secrets 权威（Secrets authority）或 Runtime 控制平面（control plane）。

## 命名规则

> **独立边界用 `delta-*`，内部职责用语义名；品牌化用于识别产品边界，不用于包装每一个内部模块。**

示例独立边界：

```text
Delta / Delta.exe
delta-runtime
delta-protocol
delta-capability
delta-worker
```

内部继续使用：

```text
session
approval
ledger
artifact
validation
memory
skill
```

只有形成独立可执行程序（executable）、公共 API、版本兼容、跨语言协议、SDK 或发布单元时，才创建新的 `delta-*` / `@delta/*` 名称。

详见 [repository-layout.md](docs/architecture/repository-layout.md)。

## R6 当前状态

R6 正在执行。PR #203 已落地 Rust RuntimeHost 与 Tauri 直接 IPC；Desktop 主路径不再依赖 Python sidecar。

仍需迁移：Python Application Control Plane、TurnEngine、Provider fallback、Capability Workerization 与持久化的 Python 后端（persistent Python backend）。

## 提交信息（Conventional Commits）

- 提交（即 Squash merge 后的 PR 标题）使用 `type(scope): summary` 格式，类型限定 11 种：`build`、`chore`、`ci`、`docs`、`feat`、`fix`、`perf`、`refactor`、`revert`、`style`、`test`。
- 破坏性变更用 `!`（如 `feat(api)!: drop the v1 protocol`）或正文中的 `BREAKING CHANGE` 脚注声明；安全、迁移等以属性标注（`feat [security]:`）。
- 摘要使用英文工程文本；非 ASCII、`Merge`/`Revert` 以外的裸消息会被拒绝。
- 权威定义见 `fongap-labs/action-worker` 的 `docs/04_CHANGELOG_CONVENTIONS.md`；本仓库与中心 PR 门禁都执行同一套规则，本地的 `.githooks/commit-msg` 只是提前反馈。

本地安装消息校验：

```bash
git config core.hooksPath .githooks
# 或者使用 pre-commit（已包含 commit-msg 阶段的同一校验）：
pre-commit install --hook-type commit-msg
```

## 分支与 PR

常规开发从 `main` 创建分支：

```text
feat/*
fix/*
refactor/*
ci/*
chore/*
docs/*
test/*
release/*
```

所有进入 `main` 的变更都通过 Pull Request 完成；默认使用 Squash merge。

Runtime / Authority 相关的 PR 必须说明：

```text
Authority Before
Authority After
Compatibility
Exit Condition
Failure / Rollback
Tests
```

不得通过降低测试标准、扩大豁免范围或恢复双重权威来让 CI 通过。

## 审查重点

- 是否符合三个产品域；
- 是否优先使用 Capability / Skill；
- Authority 是否单一、清晰；
- Worker / Skill / Learning 是否绕过 Trust；
- 是否引入不必要的 Runtime / service / protocol；
- 命名是否符合独立边界规则；
- 测试与文档是否同步。

## 文档入口

- 产品：[DELTA_BLUEPRINT.md](docs/DELTA_BLUEPRINT.md)
- 目标架构：[target-architecture.md](docs/architecture/target-architecture.md)
- 当前 Runtime 合同：[runtime-public-contract.md](docs/architecture/runtime-public-contract.md)
- 当前结构与命名：[repository-layout.md](docs/architecture/repository-layout.md)
- Capability ABI：[capability-abi.md](docs/architecture/capability-abi.md)
- 治理索引：[docs/governance/README.md](docs/governance/README.md)
- 贡献治理：[CONTRIBUTION_GOVERNANCE.md](docs/governance/baseline/CONTRIBUTION_GOVERNANCE.md)
- 质量：[QUALITY_POLICY.md](docs/governance/QUALITY_POLICY.md)
- 依赖：[DEPENDENCY_POLICY.md](docs/governance/DEPENDENCY_POLICY.md)
- 发布：[RELEASE_POLICY.md](docs/governance/RELEASE_POLICY.md)
- 历史决策：[docs/architecture/adr/](docs/architecture/adr/README.md)
- 第三方来源：[PROVENANCE.md](PROVENANCE.md)

历史 ADR、CHANGELOG 和 Git 历史保留当时的真实用词，不为当前术语治理重写历史。
