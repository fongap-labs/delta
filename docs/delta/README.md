# Delta 首版文档与导入证据

本目录存放 Delta 自有架构、首版范围和源码导入记录。组织级命名、提交、测试归属、CI、合并和发布规则仍由中央 Action Worker 治理；本目录不授予例外。

## 当前执行状态

- 当前工作分支：`codex/delta-v1-source-import`，从 Delta 远端 `main` 的 `3effeb139e9af5198bfa5d380b1d3457caea4fcb` 建立。
- 源码基线：Harness `4b399c39a6d1479cd84ca591b4d7c88e24d38a96`；仓库外裁剪回放报告记录 334 个允许包、59 个 Harness 实验包排除，以及不应进入首版的其他包排除。
- 源码复制统计：12,807 个候选文件、约 102.6 MB；未复制未处理 Harness 全量快照、Git 元数据、忽略的本地依赖和环境文件。确切候选范围以本分支提交为准。
- 已导入 Delta 品牌资源、便携启动适配、语言适配、Worktree 与 Forge 自有模块，以及本目录中的治理资料。
- 此分支不是可发布首版：未生成当前源码的发行包；CUA、浏览器操作和 LiveShare 的非实验实现与真实验收尚未完成；其他首版能力也须绑定最终提交完成完整验收。

## 文档索引

- [架构治理](DELTA_ARCHITECTURE_GOVERNANCE.md)：模块边界、来源治理、语言、数据和首版验收要求。
- [首版开发计划](DELTA_V1_DEVELOPMENT_PLAN.md)：按阶段实施及完成条件。
- [能力清单](DELTA_CAPABILITY_MATRIX.md)：首版保留能力和各项验证样例。
- [五项功能来源评估](DELTA_FEATURE_SOURCES.md)：CUA、浏览器、Forge、Worktree、LiveShare 的实现状态与缺口。
- [实验实现排除说明](DELTA_EXPERIMENTAL_EXCLUSIONS.md)：解释“排除上游实验实现，不删除产品能力”。
- [保留功能记录](DELTA_RETAINED_FEATURES.md)：对早期误写范围的纠正指引。
- [首次导入基线](DELTA_FIRST_IMPORT_BASELINE.md)：分析基线、裁剪依据和此前发现。
- [包清单](DELTA_PACKAGE_INVENTORY.json)：静态包盘点数据，不等于最终运行闭包或验收结论。

## 证据边界

回放报告、代码构建门禁、单项测试、真实桌面验收、中央 PR 门禁和发布包验收是不同证据。只有最终 Delta 提交的完整验收证据才能支撑“首版完成”结论。此分支源码推送不代表已合并 `main`，也不代表中央门禁已通过。
