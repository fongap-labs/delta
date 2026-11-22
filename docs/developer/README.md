# 开发文档

本目录是 Delta 开发说明的分类入口。以下链接在迁移过渡期仍指向现有源文件，不意味着这些文件已移动到本目录。

## 开始开发

- [开发环境与工作流程](../development.zh.md)
- [现有测试与验证说明](../testing.zh.md)
- [新增包的操作指南](../cookbook/adding-a-package.md)
- [扩展功能示例](../cookbook/extension-cookbook.md)

## 插件与应用扩展

- [插件入门](../cordis-tutorial/index.md)
- [插件开发基础](../user/develop/basic/index.md)
- [服务与事件](../user/develop/framework/index.md)
- [LLM 适配实践](../user/develop/practice/index.md)
- [工程防御性规则](../defensive-patterns/README.md)

## 执行归属

Delta 只维护产品源码、工作区锁文件、必要的编译配置及经批准的不可外置测试。新增可外置测试、共享构建器与 CI 执行逻辑应交由 [Action Worker 的 Delta 测试包](https://github.com/fongap-labs/action-worker/tree/main/tests/packs/delta) 和受信任执行器管理。

本目录是文档导航，不创建新的本地测试框架或 CI 工作流。具体职责见 [Delta 治理入口](../../governance/GOVERNANCE.md)。

## 模块与代理指令

- [包级开发约束](../../packages/AGENTS.md)：现有模块接口、编译与中央测试归属。
- [网站维护规范](../../website/AGENTS.md)：文档投射、站点交互与生成输出。
- [存量性能基准规范](../../benchmarks/AGENTS.md)：既有测试维护与中央迁移边界。
- [Agent 决策记录规范](../../.agents/notes/AGENTS.md)：设计决策、归档和历史边界。

## 中文文档迁移盘点

- [存量文档迁移基线清单](document-migration-inventory.md)：记录三个重点目录的双语文件数量、历史封存与生成器依赖，供后续分批迁移前核对。
