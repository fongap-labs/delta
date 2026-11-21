# Delta Code

Delta Code 是 Delta 自行维护的**可选编程插件**，与 Delta 主程序位于同一 Monorepo，源代码归属 `packages/plugin/delta-code/`。

- 插件包名：`@fongap-labs/delta-code`
- 共享基础库：`@fongap-labs/delta-plugin-shared`
- 装载协议：沿用 Harness/Cordis 的 `dsh.bundle`，不建立第二套插件管理器
- 工具调用接口：保留原有 `code_*` 名称，以便后续兼容迁移
- 源码基线：`fongap-labs/dsh-plugin@f6f265ae211cfa99830d214c3fba3319e5a5ce1c`

## 当前交付边界

本包仍设置 `private: true`，首先用于仓库内编译、中央合同测试与真实宿主兼容验收。**不能因为源码已迁入，就认为已发布、默认启用或可供用户稳定安装。**

独立插件发行前需要先完成旧包及旧注册 ID 的配置迁移、Cordis 装载和停用卸载验证、主程序兼容性及平台回归。不能与旧插件同时注册同名 `code_*` 工具。历史包名与注册 ID 仅可在迁移器和迁移记录中引用，新安装和运行时统一使用 `delta-code`。

## 内部命名与历史兼容

- 插件包名、Cordis ID、设置表单、系统提示区块、日志前缀及 GitHub User-Agent 统一为 `delta-code`。
- 新 GitHub 凭据环境变量默认以 `DELTA_CODE_GITHUB_` 开头。已经保存在用户配置中的旧环境变量引用不会被自动改写，避免凭据失效。
- 新管理的工作树使用 `.delta-code/worktrees/`。旧工作树仍留在其原有位置；仅在明确执行移除操作且通过现有 Host/Git 验证、工作树清洁检查后，才允许按历史路径安全移除。**不会自动移动或删除旧用户数据**。
- 旧配置迁移仍属于独立验收项，不能因本次内部命名统一就宣称升级兼容已完成。

## 权限与安全

代码、Git、LSP、工作树和 GitHub 写入属于较高权限能力，不默认向普通 Delta 用户开放。GitHub 写入应继续采用明确的关闭默认值。

命令通过宿主已有的 subprocess 服务执行；宿主服务缺失或不可用时拒绝执行，**不回退到 `node:child_process`**。工作区词法路径检查不构成 OS 级文件隔离或符号链接防护，不能替代宿主授权、安全确认与真实运行时验收。

## 维护与治理

`packages/plugin/shared/` 维护可被多个第一方插件复用的能力；具体业务保持在本插件。源码和产品特有约束以 Delta 的 `governance/REPOSITORY.md` 为准；CI、测试、合并与发布一律以 [Action Worker](https://github.com/fongap-labs/action-worker) 中央治理为准。

对应迁移任务：[Delta Issue #14](https://github.com/fongap-labs/delta/issues/14)。


## 旧用户 Profile 迁移预检（只读）

维护人员可以对现有 Harness/Delta Profile 目录执行以下检查，确认是否仍引用旧包和旧 Cordis ID：

    node scripts/delta-code-profile-audit.mjs /absolute/path/to/profile

输出只有固定布尔值、数量和状态，不打印配置值、Token、环境变量内容或原始 YAML；程序只读取 Profile 的 package.json 与 cordis.patch.yml，不会改动文件，也不会执行 YAML 内的 !!js 表达式。

- **absent / current-only**：退出码 0；只代表这两个文件没有检测到旧插件，不代表已验收实际装载。
- **legacy-only**：退出码 1；需要使用现有插件管理器完成停用旧 bundle、核查配置和后续迁移。新插件仍为私有工作区包，不得直接作为正式插件向用户发布。
- **mixed**：退出码 2；旧包或注册条目与新包并存，拒绝自动替换，以免 code_* 工具重复注册。
- **invalid**：退出码 2；输入缺失、格式错误或不可读取时明确失败，不假装迁移安全。

此预检**不扫描**其他 Home/Invocation overlay、锁文件及实际插件运行状态，也不能取代安装、禁用、卸载与数据迁移测试。正式迁移必须通过 Harness 原生插件管理器受控执行，保留原配置备份，并核验其他叠加层是否仍使用旧注册 ID。不得自动删除旧工作树或改写用户已保存的 Token 引用。
