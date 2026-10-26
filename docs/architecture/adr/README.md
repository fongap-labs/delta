# 架构决策记录（ADR）

本目录保存 Delta 的架构决策记录。每份 ADR 说明一项决定的背景、决定内容与后果。下表列出本仓库中保存的决策；"代码中引用但本仓库未保存的编号"一节说明源代码注释中仍在引用的旧编号体系。

## 本仓库中的决策

| ADR | 标题 |
|---|---|
| [0001](0001-one-delta-one-core.md) | 唯一的 Delta，唯一的核心 |
| [0002](0002-foundation-suite-boundary.md) | Foundation 与扩展边界（文件名为历史遗留） |
| [0003](0003-capability-over-fork.md) | 以能力代替分叉 |
| [0004](0004-capability-execution-chain.md) | 能力标准执行链 |
| [0052](0052-credential-storage.md) | 凭据存储（第一步已实现，第二步待所有者决定；编号 0052 以避免与代码中引用的 ADR-005 ... ADR-051 冲突） |
| [0053](0053-resume-run-identity.md) | 恢复运行沿用哪个运行身份 |
| [0054](0054-statistics-engine.md) | 哪个统计引擎产出可报告的数字 |
| [0055](0055-numeric-traceability.md) | 报告中的每个数字都可追溯到计算结果 |
| [0056](0056-study-record.md) | 多轮研究的设计与决策保存在哪里 |
| [0057](0057-workspace-sensitivity-model-egress.md) | 工作区敏感度与可发送到模型端点的内容 |
| [0058](0058-baseline-work-capabilities.md) | 基础办公能力归属 Foundation |
| [0059](0059-computer-capabilities.md) | 电脑能力，分步实现（截图、剪贴板、打开；操作屏幕尚未决定） |
| [0060](0060-controlled-browser.md) | 供 Agent 使用的受控浏览器（经 DevTools 驱动 Edge 或 Chrome；独立配置文件；与 `web_fetch` 相同的网络守卫） |
| [0061](0061-egress-level-outbound-tools.md) | 工作区出境档位同样约束对外工具 |
| [0062](0062-desktop-input-control.md) | 桌面输入控制的同意机制（提议；按程序授权、敏感动作逐次确认；取代 ADR-0059 第三步的设计部分） |
| [0063](0063-edit-existing-office-files.md) | 编辑已有的 Office 文件（有边界的原地编辑；修订 ADR-0058 的范围一条） |
| [0064](0064-completion-notices-and-webhook.md) | 任务完成通知与可选 Webhook（出站判定与模型请求相同；默认只发最少内容） |
| [0065](0065-approval-ladder.md) | 渐进授权阶梯（单次、会话、工作区；授权不推翻拒绝；可见可撤销） |
| [0066](0066-builtin-content-skills.md) | 随产品发布的内容创作技能（公众号文章、五图图文笔记、短视频素材包；只读，可复制） |
| [0067](0067-local-library.md) | 本地资料库与全文检索（FTS5、三元组分词；命中带位置与 SHA-256；不含向量层） |
| [0068](0068-data-analysis-skill.md) | 随产品发布的统计分析技能（只补现状没有的内容；办公文档不新增技能，已有则择优） |

## 代码中引用但本仓库未保存的编号

源代码注释与文档中还引用了旧编号体系中的 ADR（`ADR-005` ... `ADR-051`）。这些决策文档**不在本仓库中**；下表只记录引用处所说明的主题，便于贡献者理解注释指向什么。以代码及 `docs/architecture/runtime-public-contract.md` 为事实来源。当其中某项决策以当前 `NNNN-title.md` 格式写出后，应将其加入第一张表，并从本表删除。

| ADR | 引用处所述主题 | 引用位置 |
|---|---|---|
| ADR-004 | 取消托管 OAuth，改为手动连接 | `apps/desktop/src` |
| ADR-005 | 可靠的任务运行时（含 WS3：Validation） | `crates/delta-core/src/artifact.rs`、`validation.rs` |
| ADR-009 | Delta 核心架构 | `crates/delta-core/src/lib.rs` |
| ADR-019 | R2 预埋：成果物注册表的影子读取器，验证的影子校验器 | `artifact.rs`、`validation.rs` |
| ADR-020 | 成果物注册表的 Rust 写入权威 | `artifact.rs` |
| ADR-022 | Rust 权威的工具幂等状态机 | `tool_lifecycle.rs` |
| ADR-024 | 硬切换：Rust 是任务存储的唯一权威 | `taskstore.rs` |
| ADR-027 | R2 来源 / 引用硬切换 | `source_citation.rs` |
| ADR-028 | 验证可信事实的写入权威 | `validation.rs` |
| ADR-029 | 检查点权威（`checkpoint.registered` 事件） | `lib.rs` |
| ADR-030 | 策略权威（工具调用风险等级、策略切片） | `lib.rs` |
| ADR-031 | 审批权威（审批审计事件） | `lib.rs` |
| ADR-037 | 取消决定权威 | `tool_lifecycle.rs` |
| ADR-038 | 超时决定权威 | `tool_lifecycle.rs` |
| ADR-039 | 重试策略决定权威 | `retry.rs` |
| ADR-042 | 运行生命周期状态机与检查点 | `ledger.rs`、`runtime.rs` |
| ADR-044 | `next_run` 由纯 Python `compute_next_run()` 计算 | `taskstore.rs` |
| ADR-051 | Rust 核心控制平面权威 | `lib.rs` |
