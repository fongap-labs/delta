<div align="center">

<img src="resources/brand/delta-logo-256x256.png" width="112" alt="Delta 标志">

# Delta

本地优先的个人 AI 工作助手，专注于日常办公、研究分析与内容创作。

[![License](https://img.shields.io/badge/License-Apache%202.0-286F78?style=flat-square&labelColor=1D3F45)](LICENSE) [![Python](https://img.shields.io/badge/Python-3.12-3E7F86?style=flat-square&labelColor=1D3F45)](.python-version) [![Node.js](https://img.shields.io/badge/Node.js-22-4E7C6F?style=flat-square&labelColor=1D3F45)](.nvmrc) [![Rust](https://img.shields.io/badge/Rust-stable-8C6D3F?style=flat-square&labelColor=1D3F45)](rust-toolchain.toml) [![Tauri](https://img.shields.io/badge/Tauri-2-2D5C6B?style=flat-square&labelColor=1D3F45)](apps/desktop) [![Status](https://img.shields.io/badge/Status-pre--release-7A5C2E?style=flat-square&labelColor=1D3F45)](CHANGELOG.md) [![Release](https://img.shields.io/github/v/release/fongap-labs/delta?label=Latest&display_name=tag&sort=semver&style=flat-square&labelColor=1D3F45)](https://github.com/fongap-labs/delta/releases/latest)

[快速开始](#快速开始) · [贡献指南](CONTRIBUTING.md) · [安全策略](SECURITY.md)

</div>

> 新手请从 **这是什么** 读到 **快速开始**。维护者可直接跳到 [维护者参考](#维护者参考)。

## 新手使用路径

| 阶段 | 目标 | 必读文档/步骤 | 预计时间 |
| --- | --- | --- | --- |
| 准备 | 安装依赖并下载便携包 | 快速开始 第1-2步 | 10 min |
| 首运行 | 选择出站级别并配置第一个端点 | 快速开始 第3步 + 配置指南 | 15 min |
| 基础使用 | 完成第一次本地任务（如周报生成） | 本节「你的第一个任务」 | 20 min |
| 进阶 | 探索更多能力和工作流 | [进阶使用指南](docs/ADVANCED_USAGE.md) | 30+ min |

## 这是什么

Delta 是一个运行在你电脑上的 AI 助手（Agent，能自主规划并调用工具完成任务的程序）。你交给它一项工作，它会读取资料，产出文档、表格、报告等**交付物**（Artifact），并在关键步骤请你确认。

- **本地优先**：工作记录和文件默认保存在你的电脑上。只有你明确启用的能力才会访问外部服务。
- **可控**：对外发送内容等高风险动作，必须先经过你的审批。
- **可追溯**：每个结果都能说明来源。

| 领域 | 典型任务 |
| --- | --- |
| 日常办公 | 整理文档、处理表格与 PDF、准备演示文稿 |
| 研究分析 | 设计试验、统计分析、撰写研究报告 |
| 内容创作 | 写作与编辑、图文素材、多平台发布准备 |

## 你的第一个任务

按照以下步骤完成你的第一个 Delta 任务：

1. **首次启动**：完成安装后运行 `npm run tauri dev`（或使用便携包），Delta 会提示选择出站级别
2. **选择级别**：建议首次使用选择「仅私有（Private only）」以确保所有数据只在本地处理
3. **配置端点**：选择「仅私有」后，点击「设置」→「端点管理」→「添加端点」，添加一个本地模型端点（如 Ollama 或 LM Studio）
4. **添加密钥**：在端点配置中填写对应的 API Key 或留空（如果使用完全本地的模型）
5. **开始任务**：返回主界面，在输入框中输入：`把这周的会议记录整理成周报，并列出待办事项`（请将会议记录放在 `~/Documents/meetings/` 文件夹中）
6. **确认操作**：Delta 会解析任务并列出需要执行的步骤，在涉及文件读取或保存时会请求你的确认
7. **获取结果**：确认后 Delta 会生成周报和待办清单，保存在你指定的输出目录

> 💡 **提示**：首次使用建议选择「仅私有」级别，这样可以完全在本地测试 Delta 的能力，无需配置任何外部 API Key。

## 一个例子

假设你对 Delta 说："把这周的会议记录整理成周报，并列出待办事项"，并指定记录所在的文件夹。

```mermaid
flowchart LR
    A["你：说明任务<br/>并指定资料文件夹"] --> B["Delta：读取资料<br/>起草周报"]
    B --> C["你：确认需要批准的动作（文件读取）"]
    C --> D["Delta：处理内容<br/>生成草稿"]
    D --> E["你：确认需要批准的动作（文件保存）"]
    E --> F["Delta：完成交付物"]
    F --> G["你：接收周报与待办清单"]
    classDef brand fill:#286F78,stroke:#1D3F45,color:#FBF9F9
    classDef soft fill:#E7F0F1,stroke:#286F78,color:#1D3F45
    class G brand
    class A,B,C,D,E,F soft
```

如果某个动作需要确认（比如把周报发到外部），Delta 会先问你。

## 快速开始

Delta 提供两种使用方式：**便携包直接运行**（无需安装开发工具链）和 **从源码构建**（需安装依赖）。Windows 用户推荐使用便携包。

### 路径 A: 使用便携包（推荐，无需安装开发工具链）

便携包已内置 Rust 运行时与前端资源，**无需预装 Node.js、Rust、uv**。仅需系统满足：

- **WebView2 运行时**：Windows 10 1809+ / Windows 11 自带；若缺失，首启会提示安装（几 MB）
- **Visual C++ Redistributable**：绝大多数 Windows 已装；缺失时会报 `VCRUNTIME140.dll` 错误，需安装 [Microsoft Visual C++ Redistributable](https://aka.ms/vs/17/release/vc_redist.x64.exe)

1. **下载便携包**：访问 [Delta 发布页面](https://github.com/fongap-labs/delta/releases/latest) 下载最新的 `delta-windows-x64-portable.zip`
2. **解压运行**：解压zip文件，双击 `delta.exe` 启动应用
3. **首次配置**：启动后按照提示选择出站级别并配置端点（见下方「首次配置端点」）

> 💡 若双击 `delta.exe` 无反应或报错，请先运行 Windows Update 安装最新 WebView2，或按上述链接补装 VC++ Redistributable。

### 路径 B: 从源码构建（需要安装所有依赖）

#### 1. 安装前置工具

| 工具 | 版本 | 安装方式 |
| --- | --- | --- |
| Python | 3.12 | [python.org](https://www.python.org/downloads/) |
| Node.js | 22 | [nodejs.org](https://nodejs.org/) |
| Rust | stable | [rustup.rs](https://rustup.rs/) |
| uv | **0.12.3**（必须精确一致） | `pip install uv==0.12.3` |

另外还需要 [Tauri 的系统依赖](https://tauri.app/start/prerequisites/)，按其官方说明安装即可。

> **开始前须知**
> - `uv` 必须精确为 0.12.3。
> - 首次启动必须选择出站级别；选择之前不会向任何模型发送请求。
> - 首次编译 Rust 需要几分钟。

#### 2. 获取代码并启动

在终端中依次运行（每行一步）：

```bash
git clone https://github.com/fongap-labs/delta.git
cd delta
uv sync --locked --extra dev   # 安装 Python 依赖，严格按锁文件版本
cd apps/desktop
npm install                    # 安装前端依赖
npm run tauri dev              # 启动桌面应用（首次编译 Rust 需要几分钟）
```

应用窗口打开即表示成功。首次启动时，Delta 会请你选择**出站级别**，即 Delta 能把内容发送到哪里。选择之前，它不会向任何模型发送请求。

| 级别 | 含义 |
| --- | --- |
| 开放（Open） | 可以使用任何已配置的端点 |
| 已批准端点（Approved endpoints） | 只使用你批准过的端点，例如公司网关 |
| 仅私有（Private only） | 只使用本机或内网的端点 |
| 每次询问（Ask each time） | 每次运行前由你确认端点 |

> **端点（endpoint）**：模型服务的网络地址。

端点可以是云端服务，使用时的费用由所选服务商计费；选择『仅私有』时只使用本机或内网端点。

> 🔑 **首次配置端点**：选择「开放」或「已批准端点」后，您需要：
> 1. 在 Delta 设置中打开「端点管理」
> 2. 点击「添加端点」选择服务类型（OpenAI、Anthropic、Ollama 等）
> 3. 填入服务 URL 和对应的 API Key
> 4. 保存并设为默认端点
> 5. 重启 Delta 使配置生效

### 常见问题

- **`uv sync` 报版本错误**：运行 `uv --version`，必须显示 `0.12.3`。
- **第一次编译很慢**：Rust 首次编译需要几分钟，属正常现象。
- **平台支持**：目前已支持 Windows x64（**构建目标 `x86_64-pc-windows-msvc`**），发布产物为便携版 zip。其他系统的开发支持未在本文确认，请以维护者说明为准。**注意：所有示例和说明均基于 Windows 环境编写**。
- **找不到端点配置入口**：在 Delta 主界面左下角有齿轮图标（设置），点击进入后左侧导航栏可见「端点管理」。
- **第一句话怎么发**：Delta 主界面底部有一个大的输入框，直接输入您的任务描述即可，例如：「帮我总结这个 PDF 的主要观点」。
- **如何添加端点密钥**：在「端点管理」中选择已添加的端点，点击「编辑」可看到 API Key 配置字段。

## 进阶：Delta 如何工作

```mermaid
flowchart TB
    UI["体验层 Experience<br/>TypeScript / React：你看到的界面"] --> RT["运行时 Runtime<br/>Rust：会话、运行与规则"]
    RT --> MOD["Trust 信任 · Work 工作 · Capability 能力<br/>Automation 自动化 · Learning 学习（规划中）"]
    MOD --> SRC["原生工具 · Worker · MCP · Connector · 外部适配器"]
    classDef brand fill:#286F78,stroke:#1D3F45,color:#FBF9F9
    classDef soft fill:#E7F0F1,stroke:#286F78,color:#1D3F45
    classDef warm fill:#F4EDE0,stroke:#8C6D3F,color:#3B2E1A
    class RT brand
    class UI,MOD soft
    class SRC warm
```

- **运行时（Runtime）**：Delta 的核心。它对工作状态、审批和验证拥有**权威（Authority）**，即某件事只由一个模块负责。
- **能力（Capability）**：Delta 能调用的功能，如读取 PDF、网页搜索、统计计算。它的来源可以是原生工具、受控 Worker（独立的执行进程）、MCP（外部工具协议）或 Connector（外部服务连接器）。
- **自动化（Automation）**：Delta 能根据预定义规则自动执行任务序列，例如：读取邮箱→生成摘要→发送到 Slack。
- **学习（Learning）**：规划中，**尚未启用**。目前 Delta 不会从你的对话或确认行为中保存任何内容，也不会据此调整决策；设置页的“记忆”只是存放在本机的笔记，暂不参与对话。后续设计原则：只在你看得见并确认后写入，作为带来源的数据（而不是指令）提供给模型，并按工作区隔离。
- **Skill（技能）**：可复用的工作方法，定义步骤和验证方式，但不能改写安全规则。
- **扩展**：可选扩展只依赖公开合同；Delta 核心不依赖任何具体扩展，反向依赖是禁止的。

## 仓库结构

| 目录 | 内容 |
| --- | --- |
| `apps/` | 桌面应用（`apps/desktop`） |
| `crates/` | Rust 代码，包括运行时 |
| `core/` | Python 核心包（`pdf_support.py`、`roots.py` 等文档处理工具） |
| `integrations/`、`packages/` | 能力、连接器与共享包 |
| `contracts/`、`schemas/`、`delta_extension_api/` | 合同、JSON Schema 与扩展接口 |
| `packaging/`、`resources/`、`scripts/` | 打包、品牌资源与维护脚本 |
| `docs/` | 产品、架构与治理文档 |

## 维护者参考

Delta 的共用工程规则由 **Action Worker**（Fongap Labs 的统一 CI 与发布平台）维护。可以把它想成一个**中央质检车间**：所有改动都送到那里检查，通过后才能合并。

- **Runner（运行器）**：车间里临时分配给任务的电脑，用完即还。
- **Security Gate（安全门禁）**：第一道安全检查。
- **validate-merge**：唯一的合并许可。
- **AI Review**：AI 给出的参考意见，不替代以上任何检查。

### 发布

```mermaid
flowchart LR
    M["维护者：启动发布<br/>并输入版本号"] --> AW["Action Worker：<br/>检出不可变源码"]
    AW --> B["Windows 构建<br/>便携版 zip 与 SBOM"]
    B --> R["GitHub Release<br/>delta-v + 版本号"]
    classDef brand fill:#286F78,stroke:#1D3F45,color:#FBF9F9
    classDef soft fill:#E7F0F1,stroke:#286F78,color:#1D3F45
    class R brand
    class M,AW,B soft
```

- 版本号只来自发布时手动输入的值，仓库文件中的版本字段不参与决定。
- 发布清单位于 `.github/release.manifest.json`。SBOM 即软件物料清单，列出产物包含的组件。

### 文档

[产品蓝图](docs/DELTA_BLUEPRINT.md) · [目标架构](docs/architecture/target-architecture.md) · [扩展边界](docs/governance/REPOSITORY_BOUNDARY.md) · [ADR 索引](docs/architecture/adr/README.md) · [治理索引](docs/governance/README.md) · [CHANGELOG](CHANGELOG.md)

## 参与贡献

完整规则见 [CONTRIBUTING.md](CONTRIBUTING.md)，要点如下：

- 面向人的文档与 CHANGELOG 条目用中文；代码改动与 PR 标题用英文。
- 分支从 `main` 创建，使用 `feat/`、`fix/`、`docs/` 等前缀。
- 提交遵循 Conventional Commits，并用 `git commit -s` 签署 DCO（确认你有权提交这段代码）。
- PR 默认以 Squash merge 合并（把一个分支的所有提交压成一个再合并）。

### 本地检查

以下检查在 CI 中统一执行，本地运行用于提前发现问题。

| 命令 | 作用 |
| --- | --- |
| `python scripts/check_brand_icons.py --verify` | 检查图标是否与品牌源文件一致 |
| `python scripts/check_naming.py` | 检查文件命名 |
| `python scripts/check_repository_boundary.py` | 检查扩展边界 |
| `cd apps/desktop && npm test` | 桌面端单元测试 |
| `cd apps/desktop && npm run e2e` | 桌面端端到端测试（需要浏览器） |
| 在对应 crate 目录内运行 `cargo test` | Rust 测试 |

本仓库没有根级 `tests/` 目录。Python 合同测试位于 `fongap-labs/action-worker`，Rust 与桌面端测试与代码放在一起。

## 安全

请不要在公开 Issue 中披露漏洞。使用 GitHub 私密报告：**Security → Advisories → Report a vulnerability**。详见 [SECURITY.md](SECURITY.md)。

- 首次启动选择出站级别之前，不向任何模型发送请求。
- 对外发送内容等高风险动作需先经用户审批。

## 许可与商标

Delta 的 Foundation 源码采用 [Apache License 2.0](LICENSE)。第三方组件保留各自的许可证，见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。未修改的上游可以称为 Delta；修改后再发行须使用自有名称，详见 [商标与品牌治理](docs/governance/baseline/TRADEMARK_BRAND_GOVERNANCE.md)。

> 🔧 **依赖版本统一**：为了确保构建可重现性，所有依赖版本均已锁定：
> - Python: 3.12（匹配 `.python-version` 和 README 要求）
> - uv: 0.12.3（精确版本，匹配安装指令）
> 如需修改依赖版本，请同时更新 `README.md`、`.python-version`、`.nvmrc`、`rust-toolchain.toml` 和 `pyproject.toml` 以保持一致。