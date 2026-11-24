# Delta 首版文档与导入证据

本目录存放 Delta 自有架构、首版范围和源码导入记录。组织级命名、提交、测试归属、CI、合并和发布规则仍由中央 Action Worker 治理；本目录不授予例外。

## 当前执行状态

- 当前工作分支：`codex/delta-v1-source-import`，从 Delta 远端 `main` 的 `3effeb139e9af5198bfa5d380b1d3457caea4fcb` 建立。
- 源码基线：Harness `4b399c39a6d1479cd84ca591b4d7c88e24d38a96`；仓库外裁剪回放报告记录 334 个允许包、59 个 Harness 实验包排除，以及不应进入首版的其他包排除。
- 源码复制统计：12,807 个候选文件、约 102.6 MB；未复制未处理 Harness 全量快照、Git 元数据、忽略的本地依赖和环境文件。确切候选范围以本分支提交为准。
- 已导入 Delta 品牌资源、便携启动适配、语言适配、Worktree 与 Forge 自有模块，以及本目录中的治理资料。
- 此分支不是可发布首版：按用户要求，本轮不生成本地发行包。Delta 自有五项能力的 49 项回归检查已迁入 Action Worker 中央测试包，其中 42 项 Node 检查和 7 项 Vitest 集成检查本地通过；LiveShare Relay、Host 和客户端实现已加入源码。Windows 桌面实机、真实浏览器和 LiveShare Relay 端到端验收，以及其他首版能力的最终提交验收仍未完成。
- 源码提交 `dc52284bcf50afc192831f61b5b014d30d1d81c0` 上的 `pnpm run typecheck`（5/5）、`pnpm run build -- --scope artifacts`（8/8）和 `node scripts/delta-verify.mjs --ci` 均通过；这不代表便携包或产品验收通过。检查使用 Node `26.13.0`，pnpm 提示低于仓库要求的 `26.13.1`。Windows 开发启动尝试在准备 Primary Runtime 时因 `fetch failed` 退出，未打开应用窗口。
- Delta PR #89 仍为草稿；中央 CI 的 Secret Scan 因 `pinned import commit could not be verified` 失败。[Action Worker 修复 PR #2017](https://github.com/fongap-labs/action-worker/pull/2017) 仍未合并，因此中央门禁尚未通过。

## 文档索引

- [架构治理](DELTA_ARCHITECTURE_GOVERNANCE.md)：模块边界、来源治理、语言、数据和首版验收要求。
- [首版开发计划](DELTA_V1_DEVELOPMENT_PLAN.md)：按阶段实施及完成条件。
- [能力清单](DELTA_CAPABILITY_MATRIX.md)：首版保留能力和各项验证样例。
- [五项功能来源评估](DELTA_FEATURE_SOURCES.md)：CUA、浏览器、Forge、Worktree、LiveShare 的实现状态与缺口。
- [LiveShare 首版决策](DELTA_LIVESHARE_DECISION.md)：自托管 Relay、只读权限、限时链接和首版安全验收条件。
- [实验实现排除说明](DELTA_EXPERIMENTAL_EXCLUSIONS.md)：解释“排除上游实验实现，不删除产品能力”。
- [保留功能记录](DELTA_RETAINED_FEATURES.md)：对早期误写范围的纠正指引。
- [首次导入基线](DELTA_FIRST_IMPORT_BASELINE.md)：分析基线、裁剪依据和此前发现。
- [包清单](DELTA_PACKAGE_INVENTORY.json)：静态包盘点数据，不等于最终运行闭包或验收结论。
- [上游锁定文件](../../upstream.lock.json)：记录官方源 SHA、导入基线和尚未通过的验收项。

## 证据边界

回放报告、代码构建门禁、单项测试、真实桌面验收、中央 PR 门禁和发布包验收是不同证据。只有最终 Delta 提交的完整验收证据才能支撑“首版完成”结论。此分支源码推送不代表已合并 `main`，也不代表中央门禁已通过。
