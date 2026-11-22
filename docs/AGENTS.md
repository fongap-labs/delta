# Delta 文档维护规范

本文件约束 `docs/` 范围内的人工维护文档。先阅读 [Delta 文档中心](README.md) 并确定资料的唯一维护位置。组织共用命名、测试、CI、权限、合并及发布规则以 [Action Worker 共用治理](https://github.com/fongap-labs/action-worker/blob/main/docs/01_SHARED_GOVERNANCE.md) 和对应机器策略为准，本文件不得重定义或削弱这些规则。

## 一、文档分类

先区分教程与参考文档：教程按先决条件组织可操作步骤；参考文档只定义可查询的当前事实、约束和接口。复杂内容应链接到所属页面，而非在多处复制。

| 文档归属 | 保存内容 | 不应重复的内容 |
| --- | --- | --- |
| [用户指南](user/guide/index.md) | 安装配置、功能操作与常见问题 | 内部构建和实现细节 |
| [开发文档](developer/README.md) | 工作环境、插件开发、步骤式教程 | 运行时代码级类型定义 |
| [技术架构](architecture/README.md) | 组件关系、Agent 流程、扩展点、产品边界 | 每个包的完整 API |
| [技术参考](reference/README.md) | 类型、服务、配置、工具、子系统语义 | 重复的生成式表格 |
| [历史与兼容](history/README.md) | 已发布格式、迁移、事故和升级记录 | 当前行为的第二份定义 |
| [Delta 项目治理](../governance/GOVERNANCE.md) | Delta 特有产品边界与验收责任 | Action Worker 中央规则 |
| [Agent 决策记录](../.agents/notes/README.md) | 有长期价值的设计依据与取舍 | 已有教程、临时执行日志 |
| 模块 README | 本模块配置、契约、限制和扩展方式 | 其他模块细节、JSDoc 或生成目录 |
| `.agents/skills/` | 可按需调用的专用工作流程 | 产品或运行时契约 |

原有 `docs/cookbook/`、`docs/subsystems/`、`docs/cordis-api/`、`docs/postmortem/` 和 `docs/persistence-changes/` 等目录仍属于对应内容的权威来源；在路径、生成器、引用和历史兼容检查完成前，不为目录整齐而搬迁。已归档的 Agent Notes 是历史证据，不得当作当前实现要求。

## 二、写作约束

- 说明当前可验证的 Delta 行为，不将 Harness 旧机制、设计目标、推测或迁移计划写成已实现事实。历史理由放到决策记录、事故复盘、升级指南或 Git 提交中。
- 一项事实只有一个维护位置；上层文档描述职责与关系，低层页面定义参数、类型和边界。其他页面应链接，不复制。
- 简体中文作为人工维护说明的默认正文语言。代码标识符、命令、API 路径、配置键、错误码、第三方许可与机器协议保持原文。
- 每个自然段写在一条物理行上，由编辑器软换行；列表、表格和代码围栏按 Markdown 语法独立成行。该要求由 `verify-md-wrap` 检查。
- 对外约束要写清主体、输入、返回、失败条件、时序、权限和兼容边界。避免笼统的「优化」「支持」「稳定」和实现过程复述。
- TypeScript 的 `ts` 代码块必须能通过 `doc-typecheck`；逐字引用的声明使用 `ts type-equiv`，省略实现体的公共类声明使用 `ts public-api`，并按既有清单注册，不能随手将片段改成不受校验的普通文本。
- 已被文档描述的类型或服务发生变化时，在同一变更内更新所属 [子系统参考](subsystems/README.md)。现有 `verify-type-equiv` 与 `verify-subsystem-pages` 只能检查已纳入覆盖的内容，不自动发现所有尚未建档的新类型。
- JSDoc 与注释记录契约和必要语义，不保留调试过程、代码复述、审查对话或测试执行流水账。

## 三、生成式文档与旧版双语配对

- 生成式参考页、API 清单、配置目录、模块图及历史格式由对应源码和生成器维护。修改时同步更新生成器、生成产物与消费者，不直接手改随后会被覆盖的区域。
- Delta 文档正按目录迁移为中文单版本。已经迁移的内容维护单一 `.md`；新建人工维护的中文文档须在现有 [配对清单](../scripts/translation-pairing.manifest.json) 中按具体文件登记排除，不能将整个 `docs/` 目录排除。
- 尚未迁移的 `foo.md`、`foo.zh.md`、`foo.i18n.yaml` 三文件组合仍遵守 [存量翻译配对规范](i18n/README.md)：改动需一起更新、确认内容一致并通过 `verify-translation-pairing`。不允许通过删掉配对记录、跳过脚本或伪造翻译来让检查变绿。
- Markdown 文档单语言化不影响桌面、Web、插件使用的中文和英文运行时语言资源，也不影响固定的 API、SDK、协议或第三方许可证。
- 迁移前核查站点投射 `website/docs.ts`、文档生成器、相对链接、章节锚点、历史兼容与中央测试包；只有验证消费者无回归才删除旧版文件。

## 四、预算与校验

[文档字数预算](../scripts/doc-budgets.manifest.json) 由 `verify-doc-budgets` 校验：根 `AGENTS.md` 预算 2,080，本文预算 1,430，`docs/architecture.md` 预算 2,410，`docs/cordis-primer.md` 预算 600，`docs/testing.md` 预算 1,350，`packages/AGENTS.md` 预算 810，`packages/README.md` 预算 1,100。超标时先移动不属于本层的内容，再合并重复表述；确需提高预算必须解释并同步修改清单，不得忽略错误。

文档 PR 根据实际改动执行相应检查：`verify-md-links` 验证本地引用，`verify-md-wrap` 验证行结构，`doc-typecheck` 验证 TypeScript 示例，`doc-sync` 验证生成内容与现存翻译配对，`website:build` 验证文档网站。上述命令为现有仓库验证入口，不代表允许新建业务仓库本地测试框架；中央门禁仍由 Action Worker 执行与裁决。

## 五、提交前自查

- 页面属于正确类别，读者能从 [文档中心](README.md) 找到它，并且未复制另一份权威规则。
- 所有相对链接及锚点有效；历史引用采用可追溯的版本或 PR，避免会随分支移动而改变意义的证据。
- 删除重复资料前已确认源码、生成器、运行时代码、Skill 和网站没有继续引用；重要升级与持久化格式证据得到保留。
- 未让英文旧说明覆盖 Delta 产品边界，未误删应用中英文资源，也未把归档决策提升为现行规范。
- 已更新唯一 [变更日志](../CHANGELOG.md) 的未发布区域。每批通过中央 PR Governance、CI Evidence 与 validate-merge 后才可合并。

维护原则：先确定归属，再修复链接与生成流程，最后清理冗余内容；不为缩减文件数量而牺牲真实测试与兼容能力。
