# Delta

Delta 是基于 [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) 的桌面应用发行版。当前代码来自固定的 `baseline/deepseek-harness`，在此基础上应用 Delta 品牌与发行约定。

## 当前实现范围

- **应用名称**：Delta。保留上游 `dsh` 命令、Cordis API、插件标识和协议，以避免破坏已有插件。
- **语言**：产品界面只选择中文、英文；保留原有国际化 API，不进行语言系统重构。
- **品牌**：原始 Logo 来自 `legacy/v0.0.1` 的 `projects/delta/resources/brand/`，已复制到 `branding/legacy/`，并用于桌面图标、托盘和启动欢迎页。
- **启动页**：参照旧版品牌配色、图标和视觉层级；账号/API Key 操作保持 Harness 原逻辑。
- **运行架构**：保持 DeepSeek Harness 的 Electron + Node/TypeScript 运行时，**未迁移**旧版 Tauri/Rust 底座。
- **打包**：Windows x64 的正式目标为 `Delta-Windows-Portable.zip` 风格的可移动目录：`Delta/Delta.exe`、`Delta/App/Delta/Delta.exe`、`Delta/Data/`、`Delta/Other/AppInfo/appinfo.xml`。中央发布资产名仍为 `delta-windows-x64-portable.zip`。由旧版 Rust launcher 负责启动 Electron 程序。

**限制**：严格复制旧版 Tauri/Rust 的构建内部机制与保持 Harness 的 Electron 底座两者不能同时成立。这里保留旧版的用户可见便携包结构、启动器、产物命名和校验方式，而不是把 Harness 重写成 Tauri。当前未完成完整 Windows 集成构建、搬迁后运行及插件兼容性验收，禁止当作已验证发布版本。

## 开发和打包

需要 Node.js >= 24.9.0、pnpm 11.28.5。Windows 便携版还需要 Rust stable 和 Windows 构建工具。

```sh
pnpm install --frozen-lockfile
pnpm run build:desktop
node scripts/delta-verify.mjs
```

Windows 中央发布入口为 `packaging/portable/release-build.ps1`，接受 `-Target`、`-Version`、`-OutputDir`，通过原 Harness 构建获得完整 Electron 目录，再调用 `build-portable.ps1` 组装便携 ZIP。必须在真实的 Windows x64 runner 上验证。

## 分支规则

| 分支 | 用途 |
| --- | --- |
| `upstream/deepseek-harness` | 上游镜像，仅跟踪，不写入 `main` |
| `baseline/deepseek-harness` | 固定的干净上游代码，日常比对以此为基线 |
| `main` | Delta 产品代码，需经 Action Worker 统一治理 |
| `legacy/v0.0.1` | 历史 Delta 版本，仅供品牌和发行约定参考 |

上游版本记录见 [upstream.lock.json](upstream.lock.json)。引入新版本前需完成兼容审查，不能由定时任务自动合并。

## 开源许可

Delta 自有部分采用 Apache-2.0；Harness 上游采用 MIT，详见 [UPSTREAM_LICENSE](UPSTREAM_LICENSE)。第三方组件版权与许可参见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。

