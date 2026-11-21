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

## 5. 分支与交付

- `main`：Delta 正式产品主线，仅通过中央验证的 PR 合并。
- `upstream/deepseek-harness`：官方上游镜像，目标是只跟踪、不直接修改 Delta 主线；自动跟踪是否部署成功须单独核验。
- `baseline/deepseek-harness`：人工审核提升的固定上游基线，不自动追随镜像。
- `legacy/delta-v0.0.1`：历史存档，不是产品当前资源目录。
- `feature/*`、`fix/*`、`docs/*`：开发工作分支。

上游常规差异以基线与镜像比较，不以 Delta `main` 对官方上游的提交差异衡量二开进度。升级基线前校验 Delta 的 UI、隐私、数据、Agent、插件与便携构建。

主程序及插件独立管理制品和版本，但实际签名、构建执行、发布编排及权限仍由 Action Worker 治理。发布清单按中央合同维护，不假定尚未提交的清单已经存在。

约束解释见 [应用架构](ARCHITECTURE.md)，治理引用见 [治理入口](GOVERNANCE.md)。
