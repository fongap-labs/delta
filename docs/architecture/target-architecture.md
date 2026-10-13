# Delta 目标架构

> 本文定义 Delta 的长期目标形态。它描述稳定的产品边界、逻辑模块、语言职责和依赖方向，不等同于当前物理目录，也不把尚未完成的迁移写成当前事实。
>
> 当前实现见 `runtime-public-contract.md`、`repository-layout.md`、ADR 与 CHANGELOG。

## 1. 总体形态

Delta 是一个本地优先的个人工作 AI Agent，只围绕三个产品域：

```text
日常办公 | 研究分析 | 内容创作
```

底层是一套统一的工作系统，不是三个 Agent、三个 Runtime 或三个产品。

```text
                         Delta Desktop
                       TypeScript / React
                              │
                     Tauri Commands / Events
                              │
                              ▼
                         Delta Runtime
                              Rust
                              │
        ┌─────────────┬───────┼────────┬─────────────┐
        ▼             ▼       ▼        ▼             ▼
      Trust          Work  Capability Automation   Learning
                              │
                 ┌────────────┼────────────┐
                 ▼            ▼            ▼
              日常办公       研究分析       内容创作
                              │
                       Task Workers
                 Python / PowerShell / Shell
```

一句话边界：

> **TypeScript 负责产品交互，Rust 负责 Runtime、状态、规则与事实；Python、PowerShell、Shell 只作为受控的工作执行语言。**

## 2. 三个产品域

### 日常办公

```text
读取 → 理解 → 修改 / 创建 → 验证 → 预览 → 交付
```

覆盖文档、表格、PDF、演示材料、会议材料、邮件 / 消息、文件处理，以及正式的 Office / Markdown 成果。

### 研究分析

```text
研究问题
→ 假设 / 目标 / 约束
→ DOE（试验设计）
→ 序贯试验设计
→ 数据采集 / 清洗
→ 统计分析 / 建模 / 可视化
→ 结果解释 / 不确定性判断
→ 下一轮试验 / 停止判断
→ 正式研究报告
```

至少覆盖描述统计、统计推断、样本量 / 功效、随机化、区组、全因子 / 部分析因、筛选设计、响应面、最优设计、序贯设计、模型更新，以及正式研究报告。

序贯设计从第一版架构中就存在。Delta 可以推荐下一轮试验，但研究目标、允许调整范围、停止规则和高后果决策必须可记录、可解释，并由用户掌握最终决定。

### 内容创作

```text
选题 → 研究 → 写作 → 编辑 → 图文 / 图片 / 视频
     → 多平台适配 → 发布准备 → 数据复盘 → 下一轮优化
```

包括长短文、自媒体内容、图文、信息图、图片生成与处理、短视频脚本 / 分镜 / 字幕 / 轻量生成处理，以及发布素材。专业的媒体模型 / 编辑器可以作为受控的外部能力接入，不要求核心 Runtime 自己实现媒体引擎。

## 3. 七个长期逻辑模块

### Experience（体验层）

用户可见的交互。主要由 TypeScript / React 实现：Workspace / Session、对话、任务进度、Steer（实时调整）/ Follow-up（追加）/ Cancel（取消）、Approval（审批）、预览、编辑、Skill / Automation / Settings。

UI 不自行推导权威业务事实。

### Runtime（运行时）

Rust 负责 Session / Run / Turn（会话 / 运行 / 轮次）、Agent loop、Context / Compaction（上下文与压缩）、模型调用与流式输出（Model invocation / streaming）、工具分发（Tool dispatch）、Steering、Retry / Resume / Recovery（重试 / 恢复）的编排。

Runtime 决定下一步做什么，但不拥有无限的权限。

### Trust（信任）

Rust 负责 Policy（策略）、Permission / Risk（权限与风险）、Approval（审批）、Ledger / Audit（账本与审计）、Idempotency（幂等）、Secrets（密钥）、Sandbox（沙箱）以及权威不变量（Authority invariants）。

> **能力可以进化，权限不能自行进化。**

### Work（工作）

Rust 负责 Workspace、Source（来源）、Citation（引用）、Artifact（成果物）、Artifact Version（成果物版本）、Validation（验证）、Provenance（来源证明）以及 Deliverable（交付物）状态。

### Capability（能力）

主要的扩展面。Rust 作为 Host / Registry / Policy 边界；Native Tool（原生工具）、Worker、MCP、Connector（连接器）与 External Adapter（外部适配器）提供具体能力。

三个产品域主要通过 Capability / Skill 深化，而不是通过增加 Runtime 或 Agent 数量来扩张。

### Automation（自动化）

Scheduler（调度器）、定时 / 条件任务（Scheduled / Conditional Task）、唤醒（Wake）、收件箱（Inbox）、通知（Notification）、入站事件（Inbound Event），以及 Connector lifecycle（连接器生命周期）。Automation 不能绕过 Runtime / Trust / Work。

### Learning（学习）

Memory（记忆）、Preference（偏好）、Experience（经验）、Failure Memory（失败记忆）、Skill Candidate（候选 Skill）、Evaluation（评估）、Version（版本），以及 Promotion（晋级）/ Rollback（回滚）/ Retirement（退役）。

```text
工作：Goal → Run → Artifact
学习：Run → Feedback → Experience
进化：Experience → Skill Candidate → Evaluate → Promote / Reject
```

## 4. Skill 是主要的扩展单位

Skill 是可审查、可版本化的工作方法，可以组合以下部分：

```text
metadata
instructions
workflow
required capabilities
permissions
validation
templates
optional scripts
```

Learning 可以提出候选 Skill，但不能未经治理就把一次成功的任务提升为永久的高权限能力。

## 5. 语言边界

核心产品：

```text
Rust + TypeScript
```

工作执行语言：

```text
Python / R / PowerShell / Shell
```

R 只作为统计计算引擎，由 Rust 原生能力直接调用（ADR-0054）。

Worker 不得拥有 Session、Run State、core DB 权威、Policy / Approval、Secrets 权威、Artifact 的正式状态，或 Runtime 控制平面。

## 6. 命名边界

> **独立边界用 `delta-*`，内部职责用语义名；品牌化用于识别产品边界，不用于包装每一个内部模块。**

独立的可执行程序、crate / package、跨语言协议、SDK 或发布单元，可以使用 `delta-*` / `@delta/*`。

示例：

```text
Delta / Delta.exe
delta-runtime
delta-protocol
delta-capability
delta-worker
```

内部职责继续使用：

```text
session
approval
ledger
artifact
validation
memory
experience
skill
```

是否形成独立边界，由版本、API、发布、协议、进程和安全隔离的需求决定，而不是由"模块重要性"决定。

## 7. 目标依赖方向

Delta 优先采用模块化单体，不为逻辑模块机械地创建本地微服务。

```text
Experience (TypeScript)
        ↓
Runtime (Rust)
   ┌────┼────┐
   ▼    ▼    ▼
 Trust Work Capability
   │           │
   ├──────┐    ▼
   ▼      ▼  Worker / External
Automation Learning
```

原则：

- 变化快的 Capability / Experience 不污染变化慢的 Trust；
- Learning 不反向拥有 Trust；
- Worker 不反向拥有 Runtime；
- Automation 不绕过 Runtime；
- Work 的正式事实只由受信任的控制面登记；
- 没有充分证据，不增加新进程、新服务、新协议或顶级抽象。

## 8. 模型边界

Delta 只维护：

```text
OpenAI-compatible
Anthropic-compatible
```

复杂的 Provider 聚合、多 Key、权重、fallback、额度与区域路由，外置给兼容端点 / AI 网关。

## 9. 成功标准

最终目标不是"Rust 文件更多"，而是：

```text
TypeScript Experience
        ↓
Rust Runtime / Trust / Work
        ↓
Capability / Automation / Learning
        ↓
Controlled Worker / External
```

并且：

- 三个产品域可以持续扩展；
- Skill 是主要的扩展单位；
- Worker 不拥有核心 Authority；
- Human Control、Recovery、Ledger、Validation 保持稳定；
- Learning 能改善方法，但不能自行扩权；
- 本地核心工作不依赖不必要的外部服务；
- 架构和命名都由真实边界驱动，而不是由品牌包装驱动。
