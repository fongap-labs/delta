<div align="center">

<img src="resources/brand/delta-logo-256x256.png" width="96" alt="Delta Logo">

# Delta

**本地优先的个人工作 AI Agent**

日常办公 · 研究分析 · 内容创作

[English](README.md) · [**简体中文**](README.zh-CN.md)

[![CI](https://github.com/fongap-labs/delta/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/fongap-labs/delta/actions/workflows/ci.yml)
[![CodeQL](https://github.com/fongap-labs/delta/actions/workflows/codeql.yml/badge.svg?branch=main)](https://github.com/fongap-labs/delta/actions/workflows/codeql.yml)
![License](https://img.shields.io/github/license/fongap-labs/delta?label=License)

[目标架构](docs/architecture/target-architecture.md) · [扩展边界](docs/governance/REPOSITORY_BOUNDARY.md) · [开发治理](docs/governance/DEVELOPMENT_POLICY.md)

</div>

## 这是什么

Delta 是一个本地优先的个人工作 AI Agent，只聚焦三类工作：

- **日常办公**：文档、表格、PDF、演示材料、邮件与消息、文件处理和常规办公任务。
- **研究分析**：资料、试验设计、数据分析、建模、可视化、结果解释和研究报告。
- **内容创作**：研究、写作、编辑、图片、视频素材、多平台适配和发布准备。

Delta 不以堆叠模型或 Agent 为目标，而是把真实工作做完，同时让执行过程保持 **可控、可恢复、可验证、可追溯**。

## 为什么需要它

通用 Agent 可以完成很多单步任务，但当工作持续时间变长，状态、权限、证据、副作用和交付物如果只存在于上下文里，任务就容易变得脆弱。

Delta 把这些边界显式化：

```text
目标
 ↓
Workspace / Session
 ↓
Run
 ↓
Source / Citation
 ↓
Capability
 ↓
Artifact / Validation
```

模型负责判断“下一步怎么做”；Delta 负责工作状态、审批、恢复、来源追踪和结果验证。

## 怎么工作

```text
Experience
TypeScript / React
       ↓
Runtime
Rust
       ↓
Trust · Work · Capability · Automation · Learning
       ↓
Native Tool · Worker · MCP · Connector · External Service
```

核心原则：

- **本地优先**：核心工作状态保留在本地；只有明确使用外部能力时才访问外部服务。
- **人掌握后果**：高后果动作必须经过 Policy / Approval 边界。
- **任务可恢复**：工作可以暂停、取消、继续和复查。
- **成果可验证**：Source、Citation、Version、Provenance 和 Validation 都属于工作事实。
- **扩展有边界**：新领域优先通过 Skill / Capability 接入，不新增平行 Runtime 或 Authority。
- **模型边界收敛**：Delta 只维护 OpenAI-compatible 与 Anthropic-compatible 接口；Provider 聚合、多 Key、fallback、额度和区域路由放在 Delta Core 之外。

## 扩展模型

**Skill** 是可复用工作方法的主要扩展单位，可定义 instructions、workflow、required capabilities、permissions、validation、templates 和 optional scripts。

Capability 可以由 Native Tool、受控 Worker、MCP、Connector 或 External Adapter 提供。

可选扩展只依赖 Delta 的公开扩展契约。Delta Foundation 不依赖具体扩展实现；扩展也不拥有 Core Runtime、Trust、Work 或核心状态的 Authority。

详见 [REPOSITORY_BOUNDARY.md](docs/governance/REPOSITORY_BOUNDARY.md)。

## 仓库结构

```text
apps/          用户应用
contracts/     跨进程版本化契约
core/          受控 Capability Worker 使用的 Python 辅助模块
crates/        Rust workspace crates
integrations/  Capability、Connector、MCP、Skill、Tool 集成
packages/      公共基础与 Worker 辅助模块
resources/     品牌与产品资源
schemas/       契约 Schema
scripts/       校验与维护脚本
tests/         契约、集成与跨语言测试
docs/          架构与治理文档
```

长期核心产品边界是 **Rust + TypeScript**。Python、PowerShell、Shell 只作为受控执行环境，不承担核心 Authority。

## 开发

```bash
uv sync --locked --extra dev --extra messaging
cd apps/desktop
npm install
npm run tauri dev
```

## 文档

[产品蓝图](docs/DELTA_BLUEPRINT.md) · [目标架构](docs/architecture/target-architecture.md) · [Runtime Contract](docs/architecture/runtime-public-contract.md) · [Capability ABI](docs/architecture/capability-abi.md) · [仓库结构](docs/architecture/repository-layout.md) · [开发治理](docs/governance/DEVELOPMENT_POLICY.md)

英文文档是规范来源；简体中文 README 面向中文读者维护。

## License

Delta 自有 Foundation 源码采用 [Apache License 2.0](LICENSE)。第三方代码与资源保留其原始许可证。详见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)、[NOTICE](NOTICE) 和 [LICENSES/](LICENSES/)。
