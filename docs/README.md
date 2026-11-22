# Delta 文档中心

这里是 Delta 的正式文档导航。人工维护说明统一以简体中文为规范；上游 Harness 的英文、双语文档属于待迁移存量，不承担 Delta 的文档治理或命名权威。应用界面、机器协议和第三方许可不受文档语言整理影响。

## 文档入口

- [用户指南](user/guide/index.md)：模型配置、插件、数据管理和日常使用。
- [开发指南](developer/README.md)：开发环境、插件扩展、构建与测试说明。
- [技术架构](architecture/README.md)：Delta 产品边界、现有 Harness 运行时和关键能力。
- [技术参考](reference/README.md)：API、子系统、工具、配置及持久化格式。
- [历史与兼容](history/README.md)：升级记录、历史格式和事故资料。
- [Delta 项目专属治理](../governance/GOVERNANCE.md)：项目特有约束与中央治理入口。
- [Agent 决策记录](../.agents/notes/README.md)：设计依据与历史决策，不代替现行技术规范。

## 资料归属

| 当前资料 | 正式归类 | 过渡处理 |
| --- | --- | --- |
| `docs/user/` | 用户 | 现有中文指南保留原路径 |
| `docs/cookbook/`、`docs/cordis-tutorial/` | 开发 | 暂以导航索引组织，不移动源文件 |
| `docs/architecture.md`、系统设计说明 | 架构 | 现有成对文档保留原路径 |
| `docs/subsystems/`、`docs/cordis-api/` 与生成目录 | 参考 | 先核对生成器与引用再迁移 |
| `docs/upgrade-guide/`、`docs/persistence-changes/`、`docs/postmortem/` | 历史与兼容 | 保留格式兼容证据和历史版本 |
| `docs/i18n/` | 过渡期翻译工具资料 | 完成生成、校验迁移前暂时保留 |
| `governance/` | Delta 专属治理 | 保持独立，不复制中央工程规则 |

## 文档维护边界

具体的分类、写作、生成器、中文迁移和验收规则见 [中文文档维护规范](AGENTS.md)。

- 一项事实只有一个维护来源；其他文档使用链接，不复制规则。
- 文档治理、命名与更新日志遵循 Action Worker 的 [共用治理](https://github.com/fongap-labs/action-worker/blob/main/docs/01_SHARED_GOVERNANCE.md)、[命名规范](https://github.com/fongap-labs/action-worker/blob/main/docs/03_NAMING_CONVENTIONS.md)、[变更记录规范](https://github.com/fongap-labs/action-worker/blob/main/docs/04_CHANGELOG_CONVENTIONS.md) 及中央机器策略；Delta 只记录产品专属内容。
- 新增普通文档采用符合中央命名规则的中文单文件 `.md`，不得继续创建 Harness 风格 `.zh.md`、`.i18n.yaml` 配套副本；存量文档仅在迁移期间保持现有工具检查通过，不得通过关闭门禁完成单语迁移。
- 分类索引仅负责导航；旧文档在迁移验证完成前保留原路径。以后每批迁移需同步校验 Markdown 链接、生成器、文档站点和中央 CI。

详细的产品边界与既有源码位置见 [Delta 架构](../governance/ARCHITECTURE.md) 和 [仓库归属](../governance/REPOSITORY.md)。
