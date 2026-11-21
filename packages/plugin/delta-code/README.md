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

## 内部构建及装载验证

本插件保持 `private: true`，尚不对外发布；`package.json` 已声明现行 Cordis Bundle 补丁和 Web 客户端脚本，并分别导出 `lib/index.js`、`lib/client.js`。

工作区内编译顺序：

```bash
pnpm install --frozen-lockfile
pnpm run build:lib:host
pnpm --filter @fongap-labs/delta-plugin-shared run build
pnpm --filter @fongap-labs/delta-code run build
```

`scripts/delta-verify.mjs --ci` 会检查共享库、Host 入口、Client 入口、TypeScript 声明文件，并尝试在 Node 环境导入 Host 入口。构建与导入成功不等于用户 Profile 已安装或启用本插件；运行时安装、停用、卸载以及 Windows 宿主回归仍需独立验收。

## 运行时装载验收边界

已合并的中央 [Action Worker #1947](https://github.com/fongap-labs/action-worker/pull/1947) 将**真实 Cordis 服务接口**的生命周期验证纳入可信构建门禁；它在编译后创建独立的 Cordis Context、SystemPrompt 与 ToolsService，验证以下行为：

- 插件以 `enabled: false` 装载时不注册任何 Delta 工具。
- 开启插件且存在 Host ToolsService 时，能注册 `code_multi_edit`。
- 不提供 Host subprocess 时，`code_git_worktree` 等命令执行类工具不能注册。
- 停用后工具从原生注册器移除；相同上下文再次启用、停用不遗留旧工具。

该验证**不运行真实用户 Profile，不执行文件写入、Git 或外部请求**。它并不能代替以下后续发行验收：使用独立测试 Profile 执行安装/选择/移除、应用进程真实启动与停机、更新后配置保留、Windows 桌面和便携版安装及完整功能回归。在这些完成前，本包继续保持 `private: true`，不发布、不默认启用。

## 隔离 Profile 选择与停用验收

中央 [Action Worker #1948](https://github.com/fongap-labs/action-worker/pull/1948) 通过临时 Profile 验证 Delta Code 的 Bundle 层与启停配置。验收调用现行 `initProfile`、`loadProfileDirectory`、`composeEntries`、`writePluginEnabled`，检查：

- 已选择 `@fongap-labs/delta-code` 时只生成一个 `delta-code` 条目；
- 停用后该条目被禁用，重复停用不会重复写入；
- 重新启用后条目恢复，重复启用保持幂等；
- 取消选择该 Bundle 后，其条目不再进入组合结果；
- 全部操作只发生在临时目录，测试结束后清理，不访问用户 Profile。

测试使用指向已构建工作区包的临时符号链接模拟**已安装后的包解析**，不调用真实 pnpm 安装器。因此其成功不意味着安装、依赖拉取、卸载、桌面热重载或 Windows 发布已完成。以上仍属于后续独立验收项，插件保持私有且默认不启用。

## 唯一插件身份与数据边界

- 包名统一为 `@fongap-labs/delta-code`；Cordis 注册 ID、设置表单、提示区块、日志前缀及 GitHub User-Agent 使用 `delta-code`。
- 新 GitHub 凭据引用以 `DELTA_CODE_GITHUB_` 开头。仅使用用户在当前插件配置中明确提供的凭据引用，不提供旧引用转换、别名或自动迁移。
- 管理的工作树**只使用** `.delta-code/worktrees/`。创建和移除均不搜索其他历史目录，移除必须通过既有 Host/Git 路径、注册状态及工作树清洁检查。
- 工作树管理功能不会主动识别、迁移、移动或删除历史插件目录。用户磁盘上已有的旧数据原样留存，但不属于 Delta Code 的受管理工作树范围。
- 仅保留 Harness/Cordis 当前装载协议 `dsh.bundle`；这是与主程序的运行协议，不是旧插件兼容层。

## 权限与安全

代码、Git、LSP、工作树和 GitHub 写入属于较高权限能力，不默认向普通 Delta 用户开放。GitHub 写入应继续采用明确的关闭默认值。

命令通过宿主已有的 subprocess 服务执行；宿主服务缺失或不可用时拒绝执行，**不回退到 `node:child_process`**。工作区词法路径检查不构成 OS 级文件隔离或符号链接防护，不能替代宿主授权、安全确认与真实运行时验收。

## 维护与治理

`packages/plugin/shared/` 维护可被多个第一方插件复用的能力；具体业务保持在本插件。源码和产品特有约束以 Delta 的 `governance/REPOSITORY.md` 为准；CI、测试、合并与发布一律以 [Action Worker](https://github.com/fongap-labs/action-worker) 中央治理为准。

对应迁移任务：[Delta Issue #14](https://github.com/fongap-labs/delta/issues/14)。
