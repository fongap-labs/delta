# Delta 包级开发约束

本文件补充[仓库级指令](../AGENTS.md)与[中文文档规范](../docs/AGENTS.md)，仅约束 `packages/` 下的模块实现。通用测试、构建、命名、CI、发布和合并以 [Action Worker 共用治理](https://github.com/fongap-labs/action-worker/blob/main/docs/01_SHARED_GOVERNANCE.md) 及中央机器策略为准，不在此另建执行器或例外。

## 模块与接口

- 服务类插件默认导出服务类；函数式插件具名导出 `name`、`inject`、`Config`、`apply`，不得混用默认导出，否则 Loader 可能丢失注入声明。参见[历史复盘](../docs/postmortem/0001-acp-default-export-drops-inject.md)。
- 可选服务通过 `ctx.get(name)` 访问；`ctx.<name>` 仅用于明确声明的注入，不能依赖其随拓扑变化的代理行为。
- 模型可见能力应经过真实 Loader、应用或进程组合验证，不能只用手工 `ctx.plugin(...)` 单元测试代替。验证模型可见、用户可见或可持久化的输出；外部服务及不可确定输入可使用隔离替身。真实测试由中央或已批准的存量测试完成。
- `ctx.agents.withInitiator()` 范围内，每次编排入口恢复 Agent 并派生 `agent.session`；生命周期、Session 日志、服务、进程、持久化和传输边界显式传递 `Agent` 或 `Session`，不为隐藏参数而向叶子函数注入整个 Context。
- 单个异步操作应有明确的生命周期控制器或事务；就绪、取消、清理与提交若无独立所有者，应合并状态并保证回滚和最终收敛。
- Service Definition 满足当前所有 Consumer 的需求；工具 Schema、Loader、UI、传输和 Provider 特有行为放在 Consumer 或 Provider，不让某个调用方决定公共服务契约。只有一个内部调用方的方法优先保留为私有能力闭包。
- 新增抽象、状态机、选项和兼容分支必须有当前业务调用方或可验证契约；没有证据时要求调用者显式提供值，避免无根据默认。
- 共享状态归明确的服务或实例所有；不得新增模块级全局缓存、`WeakMap`、`Symbol.for`、`globalThis` 或 `window.__X__` 来隐藏生命周期与作用域。
- 业务策略放在相应业务包内；运行时、Controller、Manager、Gateway 和 API 分发层只负责编排。跨包能力通过注册接口或数据投影连接，参见[架构归属](../docs/architecture.md#where-new-behavior-goes)。
- Prompt、工具 Schema、结果与诊断以模型任务视角描述，避免泄露 UI、传输和实现术语。稳定的模型可见文本与动态行为应由中央验收或已有快照验证。
- 权限拒绝和其他安全决策必须在真正执行操作的位置强制执行；仅隐藏 Schema、过滤 Prompt 或增加包装器不能防止直接调用。
- 仅在成功提交后发布状态、通知和派生视图；缓存、UI、会话重放和搜索查询共享同一权威数据源。
- 用户搜索遵守[客户端搜索与过滤规则](client/AGENTS.md#search-and-filter)，包含 Host 搜索为 Client 列表供数的场景。
- 字节数、Token、条目数和时限应对最终发出或保留的完整结果（含包装和元数据）限额；注意极小限制、边界值及多字节字符。
- Registry 注册项必须在释放 Fiber 后正确注销；进程、端口、路径等资源在失败时也必须由所有者清理。

## 编译和文档

- 包的 `tsconfig` 继承 `tsconfig.base.json`，客户端继承 `tsconfig.base.client.json`；源码目录通常为 `src`，输出类型目录为 `lib/types`，工作区依赖必须声明编译引用。真正需要 Host/Client 分离的包使用 `tsconfig.host.json` 和 `tsconfig.client.json`；详见[TypeScript 项目布局](../docs/development.md#typescript-project-layout)。
- `src/types.ts` 仅放类型声明，不放运行时代码。
- 包名和文件命名遵守[中央命名规则](https://github.com/fongap-labs/action-worker/blob/main/docs/03_NAMING_CONVENTIONS.md)以及[当前包角色](../docs/cookbook/adding-a-package.md#name-the-role-that-exists)。
- 行为变更同时更新包 README 和 JSDoc，并核对[文档写作要求](../.agents/skills/dsh-prose-standard/SKILL.md)。组级 README 需说明子系统归属或既有例外，不复制生成式事件与工具目录。
- 包 README 按[既有格式](../docs/cookbook/adding-a-package.md#4-write-the-package-readme)描述模型体验和限制；保留要求的 `## Known Limitations and Deferred Work` 标题，确无内容时遵守现有受控允许清单。
- 现有单测、HMR 清理验证、真实组合测试和并发隔离仍属于不能随意删除的存量验证资产；本地测试须能独立获取并释放自己的端口、子进程和临时目录。

## Action Worker 测试归属

新的可外置测试必须提交到 [Action Worker Delta 测试包](https://github.com/fongap-labs/action-worker/tree/main/tests/packs/delta)，而不是在 Delta 新建 `tests/`、`*.test.*`、本地测试执行器或 CI 工作流。确实无法外置的产品专属测试，仅以中央[工程所有权策略](https://github.com/fongap-labs/action-worker/blob/main/policies/engineering-ownership.json)列明的仓库和路径为例外。

旧版 Harness 遗留测试是待收敛存量，不构成新增测试授权。只有先记录基线，在中央测试包完成等价真实运行并确认覆盖不减少，才能删除原件；不得通过空测试、降低断言或修改文档绕过门禁。
