# Delta

Delta 是基于 [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) 的桌面应用发行版。当前代码来自固定的 `baseline/deepseek-harness`，在此基础上应用 Delta 品牌与发行约定。

## 当前实现范围

- **应用名称**：Delta。保留上游 `dsh` 命令、Cordis API、插件标识和协议，以避免破坏已有插件。
- **语言**：产品界面只选择中文、英文；保留原有国际化 API，不进行语言系统重构。
- **品牌**：原始 Logo 来自 `legacy/v0.0.1` 的 `projects/delta/resources/brand/`，官方母版现统一保存在 `resources/brand/`，用于生成桌面图标、托盘和启动欢迎页资源。
- **启动页**：参照旧版品牌配色、图标和视觉层级；账号/API Key 操作保持 Harness 原逻辑。
- **运行架构**：保持 DeepSeek Harness 的 Electron + Node/TypeScript 运行时，**未迁移**旧版 Tauri/Rust 底座。
- **打包**：Windows x64 的正式目标为 `Delta-Windows-Portable.zip` 风格的可移动目录：`Delta/Delta.exe`、`Delta/App/Delta/Delta.exe`、`Delta/Data/`、`Delta/Other/AppInfo/appinfo.xml`。中央发布资产名仍为 `delta-windows-x64-portable.zip`。由旧版 Rust launcher 负责启动 Electron 程序，并通过 `DSH_HOME` / `DSH_AGENTS_HOME` 把 Harness 数据重定向到 `Delta/Data`。

**限制**：严格复制旧版 Tauri/Rust 的构建内部机制与保持 Harness 的 Electron 底座两者不能同时成立。这里保留旧版的用户可见便携包结构、启动器、产物命名和校验方式，而不是把 Harness 重写成 Tauri。Action Workers 已在受控历史迁移验收中完成一次真实 Windows x64 便携 ZIP 构建和打包运行检查；这**不等于**覆盖所有用户机器环境、插件兼容性与正式发布验收，未经完整发布门禁仍不得当作已正式验证的发布版本。

## 文档

项目说明入口：[Delta 中文文档中心](docs/README.md)。Delta 专属治理边界见 [治理入口](governance/GOVERNANCE.md)，共用工程规则由 [Action Worker](https://github.com/fongap-labs/action-worker/blob/main/docs/01_SHARED_GOVERNANCE.md) 统一维护。

## 开发和打包

需要 Node.js >= 24.9.0、pnpm 11.28.5。Windows 便携版还需要 Rust stable 和 Windows 构建工具。

```sh
pnpm install --frozen-lockfile
pnpm run build:desktop
node scripts/delta-verify.mjs
```

Windows 中央发布入口为 `packaging/portable/release-build-package.ps1`，接受 `-Target`、`-Version`、`-OutputDir`，通过原 Harness 构建获得完整 Electron 目录，再调用 `build-portable-package.ps1` 组装便携 ZIP。必须在真实的 Windows x64 runner 上验证。

Windows **CI 验收**与正式发布是两条独立流程。`.github/execution-manifest.json` 只声明中央受审计的 `windows-standard` 作业；其执行脚本由 [Action Worker 的 Delta 测试包](https://github.com/fongap-labs/action-worker/blob/main/tests/packs/delta/windows-desktop-ci.ps1) 维护，Delta PR 不能提供任意 PowerShell 执行入口。该作业在 Windows x64 Runner 上检查冻结依赖、第一方共享包与 Delta Code 构建、无签名 Electron 便携目录、打包运行时冒烟，并复制目录后用隔离 `APPDATA`/`LOCALAPPDATA` 执行 `Delta.exe --version`。**必须依据具体提交的真实 Windows 和 Linux CI Evidence、PR Governance、validate-merge 结果判定是否通过**；脚本存在、作业跳过、历史构建或 Linux 通过均不能替代 Windows 成功。此项不验证真实 GUI 中英文切换、完整插件热重载、Rust 启动器的最终 ZIP 迁移、签名或公开发行，后续跟踪见 [Issue #14](https://github.com/fongap-labs/delta/issues/14)。

Windows 无签名离线 CI 必须同时选择 Windows x64、目录打包 `--dir`、明确的 `--unsigned` 和 `DELTA_PORTABLE_OFFLINE=1`；仅设置离线标志而未传 `--unsigned` 会进入要求 `.env.windows` 的发布配置路径。受信任脚本已在 [Action Worker #1960](https://github.com/fongap-labs/action-worker/pull/1960) 改用现有 `package:win:x64:dir -- --unsigned` 入口。CI 不创建本地发布配置文件、不加载签名或更新服务凭据，仍须完整执行真正的打包与移动目录启动检查。 已合并的 [Action Worker #1961](https://github.com/fongap-labs/action-worker/pull/1961) 为 Windows 构建失败增加固定阶段诊断标志；该诊断不改变测试通过标准，失败时仍必须保留红叉并修复实际问题。

## 分支规则

| 分支 | 用途 |
| --- | --- |
| `upstream/deepseek-harness` | 上游镜像，仅跟踪，不写入 `main` |
| `baseline/deepseek-harness` | 固定的干净上游代码，日常比对以此为基线 |
| `main` | Delta 产品代码，已继承固定 Harness 基线的 Git 提交历史，需经 Action Worker 统一治理 |
| `legacy/v0.0.1` | 历史 Delta 版本，仅供品牌和发行约定参考 |

上游版本记录见 [upstream.lock.json](upstream.lock.json)。引入新版本前需完成兼容审查，不能由定时任务自动合并。

## 开源许可

Delta 有权授权的第一方代码采用 [Apache-2.0](LICENSE)，根项目及 Delta 第一方插件以此声明；保留的 DeepSeek Harness 原始代码仍遵循 [MIT](UPSTREAM_LICENSE)，不得把上游版权或许可视为已被替换。独立依赖、`vendor/` 源码及其他第三方内容保留各自许可证与版权说明，详见 [第三方许可清单](THIRD_PARTY_NOTICES.md)。


## 隐私约束（Delta）

Delta 默认运行中不装配 Session-log 的 DeepSeek 请求上传扩展、OTLP Session/产品遥测或桌面崩溃报告写盘。模型请求不附带 Harness 的匿名用户 UUID 或 Session ID 追踪请求头。基本会话持久化仅服务于本地会话恢复，不属于上传日志。第三方插件和模型服务仍可能按其自身协议处理实际请求内容，应单独审计。

Delta 保留用户主动触发的 Session ZIP 下载、Markdown 复制以及 `/export` 命令；这些操作由本机 Web/桌面 Host 的同源受保护接口完成，不连接遥测收集端点。Session 日志自动上传和匿名跟踪仍然禁用。

桌面更新操作日志模块已从产品中移除；崩溃、启动和恢复错误在本地显示时先执行凭据、路径、邮箱和 URL 的脱敏处理。正常应用诊断只输出脱敏后的有限长度文本，不收集会话内容。

### 代码层隐私治理

Delta 的第一方遥测、产品行为采集、匿名机器或用户标识、自动 Session 请求日志上传、插件清单自动披露、账号通知事件 POST、崩溃诊断文件及更新操作日志已从产品源码、装配、相关工作区包和锁文件中删除，不再提供这些模块的设置开关或空实现。模型 API 扩展注册明确拒绝 `dsh_session_log` 与 `dsh_plugin_packages` 两种敏感披露字段。

权限最小化原则：用户会话只为本地恢复而存储；用户主动点击时才通过已认证的同源接口导出 ZIP 或复制 Markdown，响应使用 `Cache-Control: no-store`，且不向遥测端点发送。桌面错误提示和本地控制台输出均须脱敏并限制长度，避免输出凭据、用户身份、邮箱、URL、路径和 Session ID。**用户主动导出的原始会话和附件可能包含其自身敏感信息，不能当作脱敏诊断数据转发。**

完全隐私验收仍依赖实际构建后的出站网络审计、插件能力审查和便携数据目录测试；第三方插件及所选模型服务的联网行为不能仅凭移除 Delta 第一方采集代码作出保证。
