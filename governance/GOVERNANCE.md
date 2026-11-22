# Delta 专属治理入口

## 1. 唯一权威

跨仓库工程治理只在 [fongap-labs/action-worker](https://github.com/fongap-labs/action-worker) 维护；本文件不复制或覆盖中央规则，仅定义 Delta 产品专属的架构、边界和验收责任。

优先级：中央机器合同/策略/规则 → 中央共享规范 → Delta 产品边界 → Delta 实现文档。Harness 旧文档只作为技术参考与兼容证据，不拥有命名、语言、变更记录或治理权威。Delta 可收紧产品验收，不得减弱中央要求。

中央权威文档：

- [共享治理](https://github.com/fongap-labs/action-worker/blob/main/docs/01_SHARED_GOVERNANCE.md)
- [仓库与合并治理](https://github.com/fongap-labs/action-worker/blob/main/docs/02_REPOSITORY_GOVERNANCE.md)
- [命名约定](https://github.com/fongap-labs/action-worker/blob/main/docs/03_NAMING_CONVENTIONS.md)
- [变更记录](https://github.com/fongap-labs/action-worker/blob/main/docs/04_CHANGELOG_CONVENTIONS.md)
- [接入规范](https://github.com/fongap-labs/action-worker/blob/main/docs/05_INTEGRATION_GUIDE.md)
- [发布与部署](https://github.com/fongap-labs/action-worker/blob/main/docs/06_RELEASE_DEPLOY_GUIDE.md)
- [主分支可信来源](https://github.com/fongap-labs/action-worker/blob/main/docs/07_MAIN_WRITE_GUARD.md)
- [测试包](https://github.com/fongap-labs/action-worker/blob/main/docs/TEST_PACKS.md)

治理说明统一使用中文，新增自有普通文件采用中央规定的三语义段 `kebab-case`，治理文档采用大写下划线及既有编号；平台与工具标准文件名、机器协议及第三方许可证保持原文。提交按中央 `feat`、`fix`、`chore` 类型，变更日志仅维护根目录 `CHANGELOG.md` 的中文未发布区域。中央规则变更必须先在 Action Worker 改机器策略、测试和文档，Delta 只保留项目差异，不复制第二套规则。

## 2. 仓库执行边界

Delta 已有 `.github/execution-manifest.json` 和 `.github/workflows/notify-action-worker.yml`。通知桥接只转发事件，中央重新查询并验证不可变提交；不得自行新增另一套工作流、合并器、特权发布流水线或 CI 证据签发器。

项目只维护产品源码、依赖锁文件、编译配置、必要的产品专属打包参数、运行需求声明及中央批准的不可外置测试。新增可外置测试必须进入 Action Worker 的 `tests/packs/delta/`；本地例外只能由中央 `policies/engineering-ownership.json` 批准，不能由 Delta 文档自行宣告。

原有测试与构建入口是过渡存量，并不授权复制更多；迁移采用中央测试包先具备覆盖、真实双跑通过、再清理旧文件的顺序。不得为通过门禁而删除断言、运行空测试或降低验证强度。

PR 合并和主分支发布按中央 `validate-merge`、CI Evidence、PR Governance 与 Main Write Guard 执行；禁止直接写 `main`、绕过失败检查或由 Delta 自行改变中央状态。

## 3. Delta 产品专属验收

目录与模块归属见 [仓库规范](REPOSITORY.md)，产品边界见 [应用架构](ARCHITECTURE.md)。以下只规定应验证的产品结果，具体测试实现与门禁仍由中央维护：

- 用户以自定义模型 API 完成首次配置和基本 Agent 工作流；保持中文、英文体验。
- Delta 第一方非必要遥测、追踪、自动上传与崩溃上报实现已清理并经检查；用户主动 ZIP / Markdown 导出仍可正常使用。
- 品牌母版以 `resources/brand/` 为正式来源，桌面与便携包的引用一致。
- 用户数据、凭据、会话、插件配置有明确兼容和迁移路径，不因升级无提示丢失。
- 新增插件先依赖 `packages/plugin/shared/` 公共基础；`delta-code` 仅验证当前包名、注册 ID、工具接口及宿主装载能力，不提供旧插件兼容、别名或自动迁移。
- Windows 便携发行验证真实启动、关闭、移动目录后数据位置和插件加载；不可仅以 Linux 构建成功替代最终发行验收。

共享模块提供的安全帮助函数不等于权限隔离；涉及第三方插件、系统命令、文件与网络访问的验收必须覆盖实际权限边界。

## 4. 实施顺序与变更纪律

1. 先确立本治理入口、架构与仓库职责，不在文档 PR 中迁移业务代码。
2. 单独迁移品牌资产并更新引用与校验。
3. 建立并验收插件共享包，所需可外置测试先进入中央包。
4. 以唯一 `delta-code` 身份验收第一方插件，旧插件和历史工作树不纳入兼容或迁移范围，再按需引入其他实际需要的插件。
5. 根据真实需求逐步沉淀 Kernel 与 Framework，不一次性重写 Harness 或替换 Electron。

每批变更分别通过中央 PR 门禁后合并；不把架构目标当成已完成状态。涉及中央构建、测试、命名或发布规则的变动，先修改 Action Worker 权威，再修改 Delta 源码及执行声明。

本目录仅保留 Delta 产品边界和验收依据。现有 Harness `docs/**` 双语配对检查属于待清理的历史技术约束，迁移应同步修复调用者并通过中央验收；不能继续作为 Delta 文档治理依据。
