# Delta 仓库架构

## 1. 仓库定位

`fongap-labs/delta` 是 Delta 主程序与所有第一方 Delta 插件共用的 Monorepo：源码共同管理，插件独立版本、独立构建与发布，统一接受 Action Worker 中央治理。

仓库当前承载 Harness 源码。目标结构不得误写为已存在的实现；迁移按小批次 PR 完成，不为统一命名而整体搬动上游源码。

## 2. 目标目录

```text
delta/
├── apps/                       # 主程序：desktop、desktop-host、web 等
├── packages/
│   ├── delta/                  # Delta 自有基础能力
│   │   ├── kernel/             # 最小产品内核（按需落地）
│   │   └── framework/          # 自有功能模块（按需落地）
│   ├── plugin/                 # Delta 第一方插件区
│   │   ├── shared/             # 从第一阶段建立的共享包
│   │   ├── delta-code/         # Delta Code 第一方可选插件
│   │   └── delta-*/            # 后续插件
│   ├── core/                   # 现有 Harness 核心
│   ├── llm/                    # 现有 Harness 模型运行能力
│   ├── session/                # 现有 Harness 会话能力
│   └── ...                     # 其他上游工作区，原路径保留
├── resources/                  # Delta 产品级静态资源（品牌母版位于 brand/）
├── packaging/                  # 产品专属打包描述和便携资源
├── native/                     # 现有原生能力
├── vendor/                     # 上游依赖
├── patches/                    # 必要修补
├── governance/                 # Delta 专属架构与治理文档
├── docs/                       # Delta 中文技术文档；历史上游内容分批收敛
├── scripts/                    # 已有必要产品脚本；不得另建中央执行器
├── .github/                    # Action Worker 薄桥接和执行清单
├── upstream.lock.json
└── pnpm-workspace.yaml
```

`packages/delta/`、`packages/plugin/` 是目标目录，当前可以尚未存在。已确认 pnpm 工作区包含 `packages/*/*`，可以覆盖上述二级 package 路径。不得仅为目录树美观创建空包。

## 3. 品牌与上游边界

Delta 官方品牌母版唯一维护在 `resources/brand/`，不再区分 Legacy；应用、启动页、图标与打包引用同步调整，避免维护多份品牌源文件。Git 历史分支 `legacy/delta-v0.0.1` 仅作为历史参考保留。

Delta 自研模块使用 Delta 标识；Harness 既有 `dsh`、Cordis、上游包名与插件协议标识按兼容要求保留，不进行无收益的全局重命名。对上游的修改保持最小且可追踪。

## 4. 多插件与共享模块

必须先建立 `packages/plugin/shared/`，再迁移第一方插件。共享库建议命名 `@fongap-labs/delta-plugin-shared`，提供公共宿主访问辅助、配置解析、生命周期处理、错误分类、脱敏与兼容检查，但不建立新的权限决策中心、插件管理器或业务服务。

- 每个插件一个独立 workspace package，包名建议为 `@fongap-labs/delta-<capability>`，例如 `@fongap-labs/delta-code`。
- `packages/plugin/delta-code/` 使用唯一包名 `@fongap-labs/delta-code`、唯一 Cordis ID `delta-code`；保留宿主所需 `dsh.bundle` 协议字段与当前 `code_*` 工具接口，不为历史插件提供别名或适配器。
- 插件可以依赖共享模块；共享模块不能反向依赖插件，插件之间原则上不直接依赖。
- 插件按需安装或启用，不默认将全部可选插件装进主程序；同仓不等于强制同版本或同步发行。
- 共享包变化应验证受影响插件；新插件必须核对构建闭包、权限、当前 Cordis 装载和独立版本，不承担旧插件包名、注册 ID、旧工作树目录或旧配置迁移。
- 不自动清理、重写或迁移用户磁盘上的旧插件数据；新插件仅管理 `.delta-code/worktrees/`，旧目录不属于其管理范围。Harness 宿主与上游协议兼容不等于支持旧 Delta Code 插件。
- 可外置插件测试由 Action Worker `tests/packs/delta/` 管理。未经中央例外，不在插件目录新增本地测试文件或 CI 执行器。

## 5. 分支、基线与提交历史

- `main`：Delta 正式产品开发主线。2026-11-22 经 Action Workers [受控迁移 #44929104924](https://github.com/fongap-labs/action-worker/actions/runs/44929104924)，已**正式继承** `baseline/deepseek-harness` 的 Git 提交历史，历史迁移后的首个 `main` SHA 为 `8b73554a94a115c3fc8310e882036ef887f0569c`。日常开发仍通过受保护分支和中央 CI 通过的 Squash PR。
- `baseline/deepseek-harness`：固定的、未二开的 Harness 参考提交 `2e39e70496aa02986ca6a820c5d497e816b69d7e`，**现在是 Delta `main` 的真实 Git 祖先**。升级需经人工验证，不自动覆盖 `main`。
- `upstream/deepseek-harness`：官方 Harness 的只读跟踪分支；在本次迁移核验时与 baseline 同为 `2e39e70496aa02986ca6a820c5d497e816b69d7e`，未来上游更新可能领先 baseline；它永远不得自动覆盖 `main`。
- `legacy/v0.0.1`：旧版 Delta 的冻结入口，与原分支 `legacy/delta-v0.0.1` 一同指向 `d9eb5531b4b708a96c0874712b3205be29741e73`。现有 `Protect Legacy Branches` 规则保护 `legacy/**`，不得作为日常开发分支。
- `legacy/main-pre-harness-migration-2026-11-22-v2`：迁移前最后的 Delta `main`，固定在 `4870259d11d8468d6d494f9d609e70c148416cf8`，用于审计、回滚对照。新 `main` 的 Git 文件树与这一提交完全一致（`37516875b61fd6cf149feb6a4c54a1ebea6d3945`），**没有产品代码损失**。
- `baseline/delta-main-2026-11-21`：早期临时快照，不是现在的开发基线；不再更新，完成归档确认后可清理。
- `migration/*`：单次迁移时的临时候选及实验分支；当前正式 `main` 已采用 `migration/rebase-delta-on-harness-2026-11-22-v2` 的提交。验证和留档完成后可按权限规范清理临时分支。
- `feature/*`、`fix/*`、`docs/*`：从最新 Delta `main` 创建的正常开发分支。

迁移将原 Delta 的八次代码快照按顺序重建为 Harness 固定基线的**八个后继提交**。因此刚迁移完成时 `main` 相对 baseline 的正确结果是 `Behind 0 / Ahead 8`；此后普通 PR 会增加 Ahead，不会因为历史脱节突然出现 28,264 个上游提交。不能只根据 Ahead/Behind 推断工作树有多少文件变动。

日常比较方式：

```bash
# Delta 产品相对固定 Harness 基线的提交
git log --oneline baseline/deepseek-harness..main

# Delta 产品相对共同祖先的文件变更
git diff --stat baseline/deepseek-harness...main

# 上游镜像相对固定基线的新提交
git log --oneline baseline/deepseek-harness..upstream/deepseek-harness

# 新旧 Delta main 是否保持代码一致（本次冻结快照）
git diff --exit-code legacy/main-pre-harness-migration-2026-11-22-v2 8b73554a94a115c3fc8310e882036ef887f0569c
```

上游升级始终先审查差异与兼容性，再通过正常 Delta PR 应用必要变更；保留品牌、隐私、插件兼容、便携打包和中央治理要求。单次历史迁移不构成今后绕过分支保护或强推 `main` 的许可。

主程序及插件独立管理制品和版本，但实际签名、构建执行、发布编排及权限仍由 Action Worker 治理。发布清单按中央合同维护，不假定尚未提交的清单已经存在。

约束解释见 [应用架构](ARCHITECTURE.md)，治理引用见 [治理入口](GOVERNANCE.md)。

## 文档语言与命名归属

Delta 的语言、文档治理、文件命名与变更日志严格遵循 [Action Worker 共用规范](https://github.com/fongap-labs/action-worker/blob/main/docs/01_SHARED_GOVERNANCE.md)、[命名规范](https://github.com/fongap-labs/action-worker/blob/main/docs/03_NAMING_CONVENTIONS.md) 与 [变更记录规范](https://github.com/fongap-labs/action-worker/blob/main/docs/04_CHANGELOG_CONVENTIONS.md)。Delta 仅维护本产品专属说明；上游 Harness 的文档约定没有治理效力。面向使用者、贡献者和维护者的人工维护文档统一使用简体中文，新增普通文件使用中央命名形式的单份 `.md`，治理文件保留大写下划线与既有编号，不再建立英文 `.md`、中文 `.zh.md` 与 `.i18n.yaml` 三件套。

本规则不改变桌面应用、Web 界面、插件及其运行时所需的中英文 locale、i18n 资源、配置键、命令、API/协议标识；许可证原文、第三方版权声明、上游冻结基线也不在文档单语清理范围内。

存量双语文档只因当前生成器、引用和验证机制仍消费这些文件而暂时保留，不再作为新文档命名的依据。按目录分批清点并迁移，修复相对链接、自动生成器、测试和中央 CI 契约；尚未迁移部分必须维持现有检查，禁止通过关闭门禁掩盖失效引用或遗漏清理。
