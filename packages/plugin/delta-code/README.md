# Delta Code

Delta Code 是 Delta 自行维护的**可选编程插件**，与 Delta 主程序位于同一 Monorepo，源代码归属 `packages/plugin/delta-code/`。

- 插件包名：`@fongap-labs/delta-code`
- 共享基础库：`@fongap-labs/delta-plugin-shared`
- 装载协议：沿用 Harness/Cordis 的 `dsh.bundle`，不建立第二套插件管理器
- 工具调用接口：当前 Delta Code 工具使用 `code_*` 名称；不提供旧插件包名、注册 ID 或配置别名
- 源码基线：`fongap-labs/dsh-plugin@f6f265ae211cfa99830d214c3fba3319e5a5ce1c`

## 当前交付边界

本包仍设置 `private: true`，首先用于仓库内编译、中央合同测试与真实宿主兼容验收。**不能因为源码已迁入，就认为已发布、默认启用或可供用户稳定安装。**

独立插件发行前必须完成 Cordis 装载、停用、卸载、主程序与平台回归。只接受新的 `@fongap-labs/delta-code` 包及 `delta-code` 注册 ID；旧插件不再兼容、不迁移、不提供别名，也不自动启用或卸载旧版本。

## 唯一插件身份与数据边界

- 包名统一为 `@fongap-labs/delta-code`；Cordis 注册 ID、设置表单、提示区块、日志前缀及 GitHub User-Agent 使用 `delta-code`。
- 新 GitHub 凭据引用以 `DELTA_CODE_GITHUB_` 开头。仅使用用户在当前插件配置中明确提供的凭据引用，不提供旧引用转换、别名或自动迁移。
- 管理的工作树**只使用** `.delta-code/worktrees/`。创建和移除均不搜索其他历史目录，移除必须通过既有 Host/Git 路径、注册状态及工作树清洁检查。
- 不识别、不读取、不移动、不删除历史插件存储内容。旧数据如仍存在于磁盘，属于用户自行备份和处理的历史数据，不代表应用继续支持旧插件。
- 仅保留 Harness/Cordis 当前装载协议 `dsh.bundle`；这是与主程序的运行协议，不是旧插件兼容层。

## 权限与安全

代码、Git、LSP、工作树和 GitHub 写入属于较高权限能力，不默认向普通 Delta 用户开放。GitHub 写入应继续采用明确的关闭默认值。

命令通过宿主已有的 subprocess 服务执行；宿主服务缺失或不可用时拒绝执行，**不回退到 `node:child_process`**。工作区词法路径检查不构成 OS 级文件隔离或符号链接防护，不能替代宿主授权、安全确认与真实运行时验收。

## 维护与治理

`packages/plugin/shared/` 维护可被多个第一方插件复用的能力；具体业务保持在本插件。源码和产品特有约束以 Delta 的 `governance/REPOSITORY.md` 为准；CI、测试、合并与发布一律以 [Action Worker](https://github.com/fongap-labs/action-worker) 中央治理为准。

对应迁移任务：[Delta Issue #14](https://github.com/fongap-labs/delta/issues/14)。
