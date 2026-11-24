# Delta

Delta 是一款可本地运行、可扩展的智能体应用。它源于 DeepSeek Harness 的成熟实现，同时由 Delta 自己决定产品范围、数据位置和后续演进。

## 项目状态

当前分支是 Delta 首版源码导入与适配分支，目标发行形态为 Windows x64 便携版。它不是已验收的发行版：按用户要求，本轮不生成本地发行包；Delta 自有 CUA 和浏览器操作实现已加入源码，但未完成 Windows 实机和真实浏览器验收；LiveShare Relay、Host 和客户端实现及 12 项回归检查已完成，真实 Windows Desktop 与自托管 Relay 端到端验收仍未完成。不要把此分支当作可供最终用户使用的发行包。

首版计划保留智能体、模型、会话、工作区、文件与终端工具、工具权限、插件、MCP、Skills、CUA、浏览器操作、Forge、Worktree 和 LiveShare。排除 Harness 标为实验性的实现不等于删除相应产品能力；未完成 Delta 自有或其他非实验实现和验证前，不宣称这些能力可用。DeepSeek 官方账号登录是可选项，登录失败不阻断不依赖该账号的使用。

## 目标便携目录

```text
Delta/
├── App/       Delta 桌面应用、命令行和运行组件
├── Data/      Delta 的配置、会话、附件、日志和本地运行数据
└── Other/     版本、校验值及随包说明
```

这是发行布局目标，不代表当前分支已生成便携包。实施及验收要求见 [首版开发计划](docs/delta/DELTA_V1_DEVELOPMENT_PLAN.md) 和 [架构治理](docs/delta/DELTA_ARCHITECTURE_GOVERNANCE.md)。

## 语言

第一方界面语言由应用设置决定，首版支持简体中文和英文；系统语言默认值可被用户覆盖。会话回复语言独立跟随用户明确指定或当前自然语言表达。机器协议、工具 schema、系统指令和供 Agent 阅读的操作说明使用英文。切换界面语言不会改写历史会话内容。

## 来源、品牌与许可

首版源码基于 Harness 固定提交 `4b399c39a6d1479cd84ca591b4d7c88e24d38a96` 在仓库外完成裁剪和 Delta 适配。导入记录、能力清单、实验项边界和文件清单位于 [`docs/delta/`](docs/delta/README.md)。

Delta 自有品牌资源已复制到 `apps/desktop/resources/delta-brand/`，来源为 [Delta legacy v0.0.1](https://github.com/fongap-labs/delta/tree/legacy/delta-v0.0.1)；构建不读取旧版仓库路径。

仓库保留 Delta 的 [Apache License 2.0](LICENSE)。导入的 DeepSeek Harness 源码仍保留其 [MIT 许可文本](LICENSES/DEEPSEEK-HARNESS-MIT.txt)，第三方组件按各自许可执行；见 [第三方声明](THIRD_PARTY_NOTICES.md)。
