# Delta 文档导航

本文档是 Delta 仓库技术资料的中文导航入口。各专题的详细约定由其所属文档维护；本页只提供入口，不复制运行时契约、测试策略或跨仓库治理规则。

## 产品与架构

- [整体架构](architecture.zh.md)：模块组成、运行流程与扩展位置。
- [能力边界](capability-seams.zh.md)：服务定义、提供方与消费方的职责。
- [子系统参考](subsystems/README.zh.md)：各子系统的行为约定与接口索引。
- [包目录](../packages/README.zh.md)：按能力查找工作区包及其实现说明。

## 使用与开发

- [用户指南](user/index.md)：产品使用、配置及插件操作。
- [开发说明](development.zh.md)：开发环境与贡献流程。
- [Cordis 教程](cordis-tutorial/index.md)：插件开发的分步教程。
- [新增工作区包](cookbook/adding-a-package.md)：包含验证步骤的开发手册。
- [防御性工程规范](defensive-patterns/README.md)：源代码审查中遵循的实现约束。

## 接口与生成资料

- [配置目录](config-catalog.zh.md)：可配置字段的生成参考。
- [工具目录](tool-catalog.zh.md)：工具能力的生成参考。
- [模块依赖图](module-graph.zh.md)：依赖关系及对应生成入口。
- [文档写作规则](AGENTS.md)：本仓库原有的文档分层、范围和格式要求。

生成资料与代码同步时，以各自生成器及校验结果为准，不直接修改生成区域。

## 测试与性能

- [测试说明](testing.zh.md)：项目当前的测试原则和开发验证说明。
- [性能基准示例](../benchmarks/agent-continuation/README.md)：已有性能基准的使用说明。
- [Action Worker 项目接入说明](https://github.com/fongap-labs/action-worker/blob/main/docs/05_INTEGRATION_GUIDE.md)：中央测试与合并执行入口。

现有 `benchmarks/` 继续保留。性能测试迁移需要先在中央建立等价覆盖和真实运行证据，不能仅通过调整目录名称绕过工程归属检查。

## 治理与迁移

- [中央共用治理规范](https://github.com/fongap-labs/action-worker/blob/main/docs/01_SHARED_GOVERNANCE.md)：组织级规则与合并门禁的唯一维护位置。
- [中文文档整理进度](https://github.com/fongap-labs/delta/issues/43)：分批改造及尚未完成的目录。
- [现存双语配对校验说明](i18n/README.zh.md)：尚未迁移文档所使用的历史配对机制。
- [项目总览](../README.md)：Delta 发行、技术栈和运行边界。

仓库中面向人的说明文档逐批收敛为中文；在配对检查、生成器和站点引用迁移完成前，不直接删除仍受校验约束的对应文件。应用运行所需的 `en`、`zh` 语言资源和机器接口标识不属于说明文档清理范围。
