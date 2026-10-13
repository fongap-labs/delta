<div align="center">

<img src="resources/brand/delta-logo-256x256.png" width="96" alt="Delta 标志">

# Delta

**本地优先的个人工作 AI Agent**

日常办公 · 研究分析 · 内容创作

![License](https://img.shields.io/github/license/fongap-labs/delta?label=License)

[目标架构](docs/architecture/target-architecture.md) · [扩展边界](docs/governance/REPOSITORY_BOUNDARY.md)

</div>

## 这是什么

Delta 是一个本地优先的个人工作 AI Agent，面向三个领域：

- **日常办公**：文档、表格、PDF、演示文稿、消息、文件以及日常办公事务。
- **研究分析**：资料来源、试验设计、数据分析、建模、可视化、结果解释与研究报告。
- **内容创作**：研究、写作、编辑、图片、视频素材、多平台适配与发布准备。

Delta 不以堆叠模型或 Agent 为目标，而是以完成真实工作为目标，同时让执行过程保持**可控、可恢复、可验证、可追溯**。

## 为什么需要它

通用 Agent 能完成许多有用的单步操作，但工作持续时间变长后，如果状态、权限、证据、副作用和交付物都只隐含在上下文中，任务就会变得脆弱。

Delta 把这些边界显式化：

```text
Goal
  ↓
Workspace / Session
  ↓
Run
  ↓
Sources / Citations
  ↓
Capabilities
  ↓
Artifacts / Validation
```

模型可以决定如何推进；Delta 则对工作状态、审批、恢复、来源证明（provenance）和验证保有权威（Authority）。

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
Native tools · Workers · MCP · Connectors · External services
```

核心原则：

- **本地优先**：核心工作状态保留在本地，除非某项能力明确使用外部服务。
- **人掌握后果**：高后果动作必须经过策略（Policy）与审批边界。
- **任务可恢复**：工作可以暂停、取消、恢复和复查。
- **成果可验证**：来源、引用（Citation）、版本、来源证明（provenance）与验证（Validation）均属于工作记录的一部分。
- **扩展有边界**：新领域通过 Skill 与 Capability 接入，不新建平行的 Runtime 或 Authority。
- **模型边界收敛**：Delta 只维护 OpenAI-compatible 与 Anthropic-compatible 接口；Provider 聚合、密钥轮换、回退（fallback）、额度与区域路由均位于 Delta Core 之外。

## 扩展模型

**Skill（技能）** 是可复用工作方法的主要扩展单位。一个 Skill 可以定义指令（instructions）、工作流（workflow）、所需能力（required capabilities）、权限（permissions）、验证（validation）、模板（templates）以及可选脚本（optional scripts）。

能力（Capability）可以由原生工具（Native Tool）、受控 Worker、MCP、Connector 或外部适配器（External Adapter）提供。

可选扩展只依赖 Delta 的公开扩展合同。Delta Foundation 不依赖具体的扩展实现；扩展也不拥有核心 Runtime、Trust、Work 或核心状态的 Authority。

详见 [REPOSITORY_BOUNDARY.md](docs/governance/REPOSITORY_BOUNDARY.md)。

## 仓库结构

```text
apps/          User applications
contracts/     Versioned cross-process contracts
core/          Python helpers for controlled capability workers
crates/        Rust workspace crates
delta_extension_api/  Stable Python extension API for optional extensions
integrations/  Capability, Connector, MCP, Skill, and Tool integrations
packages/      Shared foundations and worker helpers
packaging/     Windows portable packaging and launcher
resources/     Brand and product resources
schemas/       Contract schemas
scripts/       Validation and maintenance
(no tests/)   Python contract/integration suites live in fongap-labs/action-worker (tests/packs/delta); Rust and desktop tests stay beside their code
docs/          Architecture and governance
```

Delta 长期采用**小内核、大框架**架构。语言是实现手段，而非架构目标：应按模块边界选择最合适的语言；在性能、安全、功能、兼容性和可维护性不下降的前提下，减少不必要的语言与运行时。即使使用多种语言，同一职责也只能有一个 Authority。

## 开发

```bash
uv sync --locked --extra dev
cd apps/desktop
npm install
npm run tauri dev
```

Python、Node.js 与 Rust 工具链版本分别由 `.python-version`、`.nvmrc` 与 `rust-toolchain.toml` 指定。

## 发布

发布意图（release intent）由源码仓库在 `.github/release.manifest.json` 中声明。Action Worker 检出不可变的 Delta 源码，在集中解析出的 Runner（运行器）上执行声明的 Windows 构建，生成发布来源证明（provenance），验证源码与产物，并将 `delta-v<semver>` Release 发布到本仓库的 GitHub Releases。目标仓库凭据与发布 Authority 不存储在本仓库中。

## 文档

[产品蓝图](docs/DELTA_BLUEPRINT.md) · [目标架构](docs/architecture/target-architecture.md) · [Runtime 合同](docs/architecture/runtime-public-contract.md) · [Capability ABI](docs/architecture/capability-abi.md) · [仓库结构](docs/architecture/repository-layout.md)

贡献指南见 [CONTRIBUTING.md](CONTRIBUTING.md)，安全策略见 [SECURITY.md](SECURITY.md)。

## 许可证

Delta 自有的 Foundation 源码采用 [Apache License 2.0](LICENSE) 许可。第三方代码与资源保留其原始许可证。详见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)、[NOTICE](NOTICE) 与 [LICENSES/](LICENSES/)。
