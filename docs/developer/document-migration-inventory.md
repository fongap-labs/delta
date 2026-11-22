# Delta 中文文档迁移基线清单

本页是 [中文文档迁移任务 #43](https://github.com/fongap-labs/delta/issues/43) 和 [文档架构整理任务 #57](https://github.com/fongap-labs/delta/issues/57) 的一次性文件树盘点，不替代现行 [文档维护规范](../AGENTS.md)。它记录存量规模、资料归属和删除前必须核查的依赖，不把迁移计划写成已完成事实。

## 统计基线与口径

基准为 2026-11-22 的 [Delta main 提交 `3642ad13`](https://github.com/fongap-labs/delta/commit/3642ad13fdae8512efd276407a2025fe227ca495)。通过 GitHub Git Trees API 对下列三个子树执行 `recursive=1`，只统计 Git blob 路径中以 `.zh.md` 或 `.i18n.yaml` 结尾的文件；子树返回 `truncated: false`。同一目录内两类文件数量不相等，并不能直接判定某个文件失效，应逐项检查配对例外和生成来源。

| 目录 | `.zh.md` | `.i18n.yaml` | 主要性质 |
| --- | ---: | ---: | --- |
| [`.agents/`](https://github.com/fongap-labs/delta/tree/3642ad13fdae8512efd276407a2025fe227ca495/.agents) | 1,393 | 1,393 | 决策与历史记录，含封存内容 |
| [`docs/`](https://github.com/fongap-labs/delta/tree/3642ad13fdae8512efd276407a2025fe227ca495/docs) | 347 | 347 | 用户说明、参考、升级记录和生成式目录 |
| [`packages/`](https://github.com/fongap-labs/delta/tree/3642ad13fdae8512efd276407a2025fe227ca495/packages) | 384 | 366 | 包组、包说明、部分特殊或过渡资料 |
| **三个子树合计** | **2,124** | **2,106** | 不是全仓总量；不含 `apps/`、`native/` 等 |

复核时可从固定基线分别查询 [Agent 子树](https://api.github.com/repos/fongap-labs/delta/git/trees/2fed1b9fd51a2e9ee6086bc3d5a262ea1fb0325e?recursive=1)、[Docs 子树](https://api.github.com/repos/fongap-labs/delta/git/trees/bb523079132c077628befa1a3dc3db1605d515b6?recursive=1) 和 [Packages 子树](https://api.github.com/repos/fongap-labs/delta/git/trees/a3cdf0196db79fdf6470b47bcfbd7766ecec3d24?recursive=1)。后续批次必须按当时的目标提交重新扫描，不使用本快照作为实时剩余数量。

## 重点目录与处理边界

| 路径 | 中文副本数 | 配对记录数 | 处理判断 |
| --- | ---: | ---: | --- |
| `.agents/notes/archived/` | 928 | 928 | 已封存的历史证据，不可直接改写、移动或删除；必须先评估封存校验和只增不改约束 |
| `.agents/notes/implemented/` | 404 | 404 | 已实施决策；需确认当前实现和历史价值，不能批量转为当前规范 |
| `.agents/notes/proposed/`、`rejected/` | 60 | 60 | 提案与否决记录；分别遵守现有生命周期规则 |
| `docs/upgrade-guide/` | 186 | 186 | 升级与版本兼容资料，先核实用户阅读及历史版本引用 |
| `docs/subsystems/` | 66 | 66 | 子系统和生成式 API 片段混合，先确认生成器与源码契约 |
| `docs/persistence-changes/` | 59 | 59 | 历史数据格式和迁移证据，禁止仅为减少文件数而删除 |
| `packages/experimental/` | 77 | 61 | 两类文件存在数量差；先查逐文件配对与仓库裁剪后的实际消费者 |
| `packages/client/` | 75 | 75 | UI 包文档，保留应用真实中文和英文 locale 资源 |
| `packages/session/` | 20 | 20 | 会话持久化和兼容语义，检查格式版本与外部引用 |

这些数量是**文件数**而不是待提交 PR 数。`.agents/notes/README.zh.md` 另外计入子树总数，不在生命周期行内。

## 权威来源和消费者

| 资料类型 | 现有权威来源或消费者 | 删除或迁移前的检查 |
| --- | --- | --- |
| Delta 产品治理 | [项目治理](../../governance/GOVERNANCE.md)；共用工程规则以 Action Worker 为准 | 不复制中央 CI、合并、测试、发布规则 |
| 人工说明与模块 README | [文档规范](../AGENTS.md)、各 `packages/<group>/README.md` | 保留接口与配置语义，更新 Markdown 入站链接和旧标题锚点 |
| 既有双语配对 | [翻译配对清单](../../scripts/translation-pairing.manifest.json)、[过渡期配对说明](../i18n/README.md) | 仅对已验证的具体文件登记排除，不禁用整个配对校验 |
| 文档网站 | [`website/docs.ts`](../../website/docs.ts) 的来源映射和多语言路由 | 保证站点页面、别名与两个路由仍指向存在的源文件 |
| 生成式参考 | `docs/subsystems/`、生成式目录及对应 `scripts/` 生成器 | 先改生成器与消费者，再改产物；不得直接手改会被覆盖的区域 |
| 历史决策与封存 | [Agent Notes 规范](../../.agents/notes/README.md) 和封存校验机制 | 冻结的三文件记录不可按普通 README 合并；区分现行说明与历史证据 |

## 下一批实施次序

1. 对实际存在的 `packages/` 包组逐批处理人工 README，每批先核实代码目录、文档入站链接、章标题锚点、配对记录及运行时 locale；已被裁剪的上游目录必须重新核对，不得依旧目录列表直接迁移。
2. `docs/` 先处理明确人工维护、无生成器消费者的说明；`docs/subsystems/` 与站点投射联动修改，不提前删除生成器输入或双语网站路由。
3. 历史兼容文档保留版本语义。`.agents/notes/archived/` 在封存校验与允许的转换机制重新设计并得到批准前保持原样；其他 Agent Notes 先按生命周期及引用关系分批审查。
4. 每批建立目标 SHA 的真实验证证据：`verify-md-links`、`verify-md-wrap`、适用的 `doc-sync` 与生成器检查、`website:build`，并以 Action Worker 的 PR Governance、CI Evidence、validate-merge 为最终门禁；不能通过跳过断言或关闭检查合并。

本页不批准代码、测试或权限策略变更，不更改双语运行时资源。具体实施状态持续在 [#43](https://github.com/fongap-labs/delta/issues/43) 和 [#57](https://github.com/fongap-labs/delta/issues/57) 维护。
