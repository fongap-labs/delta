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

## 本地离线 pnpm 安装与移除验收

中央 [Action Worker #1949](https://github.com/fongap-labs/action-worker/pull/1949) 在隔离临时 Profile 中调用已有的 `runPluginCommand` 包管理器 API，以 `link:` 引用当前工作区构建后的 `@fongap-labs/delta-code` 私有包：

- 使用真实 pnpm 的 `add` 和 `remove`，明确指定 `--offline --ignore-scripts`；
- 安装后验证依赖清单与 `dsh.profile.bundles` 选择同步，并且只产生一个 `delta-code` Cordis 条目；
- 移除后确认依赖、Bundle 选择和组合条目全部消失；
- 测试从不使用用户 Profile，不触发远程注册表或安装脚本，完成后清理临时数据。

该验收局限于**工作区私有包的本地离线链接**，不等同于公开 npm 制品分发、跨机器安装、用户应用热更新或 Windows 桌面发行。实际运行和高权限功能仍需单独验证，插件保持私有且不默认启用。

## 原生代码编辑功能回归

中央 [Action Worker #1955](https://github.com/fongap-labs/action-worker/pull/1955) 已加入通过 Cordis Host 的真实 `code_multi_edit` 调用验收。在独立的临时工作区验证：

- `dryRun` 预演检查目标与当前内容，不得写盘；
- 一批编辑中出现工作区外路径时，默认预检必须阻止此前合法目标被部分改写；
- 指向工作区外文件的符号链接必须拒绝，外部文件不能被更改；
- 过期的 `expectedSha256` 必须拒绝写入，有效的 SHA-256 应允许正常编辑并真实落盘；
- 编辑完毕释放 Cordis 服务、插件和临时文件，不读取真实用户 Profile，也不调用 Git、外网或凭据。

该测试覆盖 Linux 本地文件路径的实际调用，不代表远程 Host FS 提供者、Windows/macOS 权限策略或完整 Git/LSP/GitHub 功能已验收。默认不自动安装插件，不兼容旧插件身份或历史配置。

## 私有插件压缩包验收

中央 [Action Worker #1951](https://github.com/fongap-labs/action-worker/pull/1951) 对当前工作区中**已经构建**的 Delta Code 运行本地离线 `pnpm pack`（禁用安装脚本，不发布到任何注册表），并核验实际生成的 `.tgz`：

- 包内必须包含 Host `lib/index.js`、Client `lib/client.js`、类型声明、Cordis `cordis.patch.yml`、中英文资源及插件图标。
- 压缩包的插件身份、版本、Bundle 与 Client 入口必须匹配源码声明；第一方共享模块和当前 Harness 依赖不能留有不可分发的 `workspace:` 版本协议。
- 包内不得夹带 `node_modules`、`.git`、环境文件等工作区私有内容。测试仅操作临时目录并在结束后清理。

**此阶段不改变 `private: true`，不会发布或自动安装插件。** 离线压缩包结构合格只是独立发行的必要条件，仍需跨机器安装、桌面真实启动/热重载、Windows 便携包回归及可追溯制品发布验收。

## 唯一插件身份与数据边界

- 包名统一为 `@fongap-labs/delta-code`；Cordis 注册 ID、设置表单、提示区块、日志前缀及 GitHub User-Agent 使用 `delta-code`。
- 新 GitHub 凭据引用以 `DELTA_CODE_GITHUB_` 开头。仅使用用户在当前插件配置中明确提供的凭据引用，不提供旧引用转换、别名或自动迁移。
- 管理的工作树**只使用** `.delta-code/worktrees/`。创建和移除均不搜索其他历史目录，移除必须通过既有 Host/Git 路径、注册状态及工作树清洁检查。
- 工作树管理功能不会主动识别、迁移、移动或删除历史插件目录。用户磁盘上已有的旧数据原样留存，但不属于 Delta Code 的受管理工作树范围。
- 仅保留 Harness/Cordis 当前装载协议 `dsh.bundle`；这是与主程序的运行协议，不是旧插件兼容层。

## 权限与安全

代码、Git、LSP、工作树和 GitHub 写入属于较高权限能力，不默认向普通 Delta 用户开放。GitHub 写入应继续采用明确的关闭默认值。

命令通过宿主已有的 subprocess 服务执行；宿主服务缺失或不可用时拒绝执行，**不回退到 `node:child_process`**。工作区词法路径检查不构成 OS 级文件隔离或符号链接防护，不能替代宿主授权、安全确认与真实运行时验收。

## 原生 LSP 命令可用性探测

中央 [Action Worker #1957](https://github.com/fongap-labs/action-worker/pull/1957) 通过真实 Cordis Host subprocess，在独立临时工作区测试 `code_lsp_probe`：

- 对可运行的本机命令执行 `--version` 并返回 `available=true`，对不存在的语言服务命令返回不可用结果；不得误报为已安装。
- 宿主 subprocess 仅准许运行测试列入清单的探测命令，且工作目录被限制为临时工作区。
- `cwd` 越界和已取消的请求必须在调用宿主进程前拒绝。
- 插件卸载后原生 ToolsService 不应残留 `code_lsp_probe`；所有临时文件与服务必须清理。

该验收仅覆盖 Linux 命令可用性探测，**不代表 LSP 语言诊断、补全、跳转、索引或跨平台编辑器完整能力**。未验证的 Windows 桌面装载和真实用户 Profile 仍属于正式发行阻断项。

## 默认权限与语言资源验收

中央 [Action Worker #1950](https://github.com/fongap-labs/action-worker/pull/1950) 将以下安全断言加入 Delta Code 的可信生产构建检查：

- 仅提供 `locale/en.json` 和 `locale/zh.json`，两种语言的插件名称和说明均有效；
- `normalizeConfig` 默认拒绝命令执行、工作树写入和 GitHub 写入，不自动绑定 GitHub 凭据；
- 共享宿主 `runCommand` 在没有 `subprocess` 服务时失败，不允许退回本机子进程；
- 配置拒绝明文 GitHub Token、跨所有者共用凭据引用和跨所有者仓库路由。

这些测试只检查打包后的运行时配置与资源；不会运行有副作用的命令、写入用户配置或发起 GitHub 请求。它们并不能代替真实应用 UI、操作系统授权、Git/LSP/GitHub 写操作及 Windows 发行验收。

**发行阻断条件：** 当前中央 Delta Linux 验收中的 Windows 作业仍为跳过，不得把其视为 Windows 通过。正式交付前需在独立的 Windows 测试环境启动打包后的 Electron 程序，验证中英文 UI 切换、隔离用户 Profile 的安装与移除、Host 授权和拒绝路径，并留存与源码 SHA 对应的制品证据。

## 原生 Git 只读工具回归

中央 [Action Worker #1956](https://github.com/fongap-labs/action-worker/pull/1956) 在临时本地 Git 仓库中，经真实 Cordis Context 与受限 Host subprocess 实际调用 `code_git_status` 和 `code_git_diff`：

- 临时仓库包含已提交的基线文件、未暂存修改和未跟踪文件，断言宿主工具返回真实状态；
- 差异查询必须包含基线与修改内容，且短 diff 不得错误报告截断；
- `../` 越界路径必须在发起 Host subprocess 前被拒绝；
- Host 测试服务只允许在临时仓库运行 `git status` 或 `git diff`；工具调用不得修改任何文件；
- 本地 Git 测试身份只在临时仓库内生成，执行结束后移除仓库与服务，不读取用户配置、上传仓库或请求网络。

这只验证 Linux 只读 Git 查询，不证明真实工作树写操作、LSP、GitHub 远程调用、Windows 桌面或完整安装交付。插件仍保持可选、私有，不兼容旧插件身份。

## 维护与治理

`packages/plugin/shared/` 维护可被多个第一方插件复用的能力；具体业务保持在本插件。源码和产品特有约束以 Delta 的 `governance/REPOSITORY.md` 为准；CI、测试、合并与发布一律以 [Action Worker](https://github.com/fongap-labs/action-worker) 中央治理为准。

对应迁移任务：[Delta Issue #14](https://github.com/fongap-labs/delta/issues/14)。
