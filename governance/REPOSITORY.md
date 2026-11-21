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
│   │   ├── delta-code/         # 原 dsh-delta-code（迁移目标）
│   │   └── delta-*/            # 后续插件
│   ├── core/                   # 现有 Harness 核心
│   ├── llm/                    # 现有 Harness 模型运行能力
│   ├── session/                # 现有 Harness 会话能力
│   └── ...                     # 其他上游工作区，原路径保留
├── branding/                   # Delta 唯一正式品牌资源目录
├── packaging/                  # 产品专属打包描述和便携资源
├── native/                     # 现有原生能力
├── vendor/                     # 上游依赖
├── patches/                    # 必要修补
├── governance/                 # Delta 专属架构与治理文档
├── docs/                       # Harness 技术文档（保留现有约束）
├── scripts/                    # 已有必要产品脚本；不得另建中央执行器
├── .github/                    # Action Worker 薄桥接和执行清单
├── upstream.lock.json
└── pnpm-workspace.yaml
```

`packages/delta/`、`packages/plugin/` 是目标目录，当前可以尚未存在。已确认 pnpm 工作区包含 `packages/*/*`，可以覆盖上述二级 package 路径。不得仅为目录树美观创建空包。

## 3. 品牌与上游边界

`branding/legacy/` 中的正式品牌资产迁至 `branding/`，不再区分 Legacy；应用、启动页、图标与打包引用同步调整，避免维护多份品牌源文件。Git 历史分支 `legacy/delta-v0.0.1` 仅作为历史参考保留。

Delta 自研模块使用 Delta 标识；Harness 既有 `dsh`、Cordis、上游包名与插件协议标识按兼容要求保留，不进行无收益的全局重命名。对上游的修改保持最小且可追踪。

## 4. 多插件与共享模块

必须先建立 `packages/plugin/shared/`，再迁移第一方插件。共享库建议命名 `@fongap-labs/delta-plugin-shared`，提供公共宿主访问辅助、配置解析、生命周期处理、错误分类、脱敏与兼容检查，但不建立新的权限决策中心、插件管理器或业务服务。

- 每个插件一个独立 workspace package，包名建议为 `@fongap-labs/delta-<capability>`，例如 `@fongap-labs/delta-code`。
- `dsh-delta-code` 迁移为 `packages/plugin/delta-code/`；保留 `dsh.bundle` 等 Harness 协议字段和既有 `code_*` 工具调用接口。
- 插件可以依赖共享模块；共享模块不能反向依赖插件，插件之间原则上不直接依赖。
- 插件按需安装或启用，不默认将全部可选插件装进主程序；同仓不等于强制同版本或同步发行。
- 共享包变化应验证受影响插件；插件迁移须核对旧安装配置、Cordis ID、构建闭包、权限与版本兼容，避免双重注册。
- 可外置插件测试由 Action Worker `tests/packs/delta/` 管理。未经中央例外，不在插件目录新增本地测试文件或 CI 执行器。

## 5. 分支、基线与提交历史

- `main`：Delta 正式产品主线。以 2026-11-21 的 `584e6c77ad424b1b388014854d2ecda13c9b2433` 所含代码为当前 Delta 自有开发基线；未来变更通过中央验证的 PR 合并，不将 `main` 重置到上游。
- `baseline/delta-main-2026-11-21`：上述 `main` 提交的固定快照，仅用于追溯此轮整理前后的 Delta 改动；约定保持指向该 SHA，不参与日常开发。
- `legacy/v0.0.1`：旧版 Delta 的冻结入口；与保留的原分支 `legacy/delta-v0.0.1` 一同指向 `d9eb5531b4b708a96c0874712b3205be29741e73`。现有 `Protect Legacy Branches` 规则对 `legacy/**` 禁止更新、删除与非快进修改，不得继续在旧版分支开发。
- `baseline/deepseek-harness`：人工审核提升的固定 Harness 上游参考版本，不是 Delta `main` 的 Git 祖先，也不是 Delta 产品开发基线。
- `upstream/deepseek-harness`：仅跟踪官方上游，不直接覆盖 `main`；自动跟踪是否部署成功仍须单独核验。
- `feature/*`、`fix/*`、`docs/*`：从最新 Delta `main` 创建的开发分支。

当前 `main` 是通过源码导入建立的**独立线性 Git 历史**，与 Harness 上游历史没有共同祖先。GitHub 在这些互不相连的分支间显示的 `Ahead/Behind` 数量不是 Delta 漏掉的更新数；不得通过伪合并、强推或绕过 `main` 的线性历史 / Squash 合并门禁来消除数字。

按需要使用工作树差异或各自历史比较，而不是用无共同祖先的三点提交比较：

```bash
git diff --stat baseline/delta-main-2026-11-21 main
git diff --stat baseline/deepseek-harness upstream/deepseek-harness
git diff --stat baseline/deepseek-harness main
```

第一项表示冻结快照以来的 Delta 主线变动；第二项表示 Harness 上游相对审核基线的变动；第三项只比较当下代码内容，不把两份独立提交历史解释成继承关系。上游升级应经人工核对、迁移和验证，再用正常 Delta PR 应用必要修改，保留品牌、隐私、插件兼容与打包要求。

主程序及插件独立管理制品和版本，但实际签名、构建执行、发布编排及权限仍由 Action Worker 治理。发布清单按中央合同维护，不假定尚未提交的清单已经存在。

约束解释见 [应用架构](ARCHITECTURE.md)，治理引用见 [治理入口](GOVERNANCE.md)。
