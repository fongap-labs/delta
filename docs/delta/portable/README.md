# Delta Windows x64 便携版使用说明

Delta 是独立的桌面智能体应用。运行时复用部分 DeepSeek Harness 组件，但不代表本应用与 Harness 的账号、插件或会话数据自动兼容。

## 启动和数据目录

将发行 ZIP 完整解压至可写目录，保持 App、Data、Other 三个同级目录。运行 `App/Delta.exe` 启动桌面应用；在 App 目录运行 `delta.cmd --help` 查看实际支持的命令和参数。Other 中的 VERSION.json 标识发行版本，SHA256SUMS.txt 用于核验包内应用和随附文件。

Data 包含 UserData、Session、Logs、Temp 和 Home，可能保存用户配置、凭据和会话内容。不要将整个 Data 目录上传至公共平台。移动便携应用前退出应用及相关进程；移动时保持三个目录的相对布局。

## 使用边界

首次启动需要可写的 Data 目录。使用在线模型需要配置相应的服务和凭据；DeepSeek 官方账号登录并非所有功能的前提。插件及 MCP 服务应按其实际兼容性和权限要求安装，不要因其来自上游生态而假定兼容。工具、命令执行和工作区访问应遵循应用权限提示。

Beta 版本用于测试，尚未完成的功能验收不视为已通过；不要将其作为唯一的生产数据副本。版本迁移和回退见 [升级与数据保护](upgrade-guide/README.md)。
