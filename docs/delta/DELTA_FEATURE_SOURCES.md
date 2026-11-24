# Delta 五项产品能力与实现来源（工作稿）

> 用户澄清：CUA、浏览器操作、Forge、Worktree、LiveShare 产品能力保留；不直接吸收 Harness 处于实验状态的功能实现。此前将五项产品功能全部排除的记录无效。本文件是源码盘点与替代路线，不代表实现或验收已经完成。

固定分析版本：Harness `4b399c39a6d1479cd84ca591b4d7c88e24d38a96`。

| 功能 | 上游固定版本发现 | 实施调查方向 | 验收重点 |
|---|---|---|---|
| CUA | `packages/experimental/cua-bundle/`、`cua-windows/` 等属于实验组；稳定包只提供能力注册表 | 已加入 Delta 自有 Windows PowerShell/Win32 provider；默认关闭，逐操作审批，截图使用短时 `screenId` 并对共享桌面操作串行化 | Node mock 检查通过；Windows 真桌面截图、点击、文字输入、拒绝和取消尚未验收 |
| 浏览器操作 | 稳定包只提供能力注册表；实验性提供方未导入 | 已加入 Delta 自有可视浏览器 provider，基于 Playwright Core 和本机 Edge/Chrome；隔离 Session profile、逐操作审批，默认关闭 | Node mock 检查通过；真实 Edge/Chrome 启动、网页打开、读取、操作和资源清理尚未验收 |
| Forge | 上游 Forge、GitHub/GitLab 提供方和面板在实验组 | 已编写 Delta 自有的只读 API 适配基础；GitHub 和 GitLab 官方公共变更请求读取通过真实请求验证，已接入凭据服务、命令与构建；真实私有账号和桌面验收待完成 | GitHub 和 GitLab 的认证、变更请求读取及错误处理 |
| Worktree | 上游工作树工具及其 Forge 组合属于实验组 | 已有 Delta 自有 Git worktree 模块，默认关闭 | 创建、进入、退出、任务隔离、并发与安全清理 |
| LiveShare | 上游 LiveShare owner 和协议在实验组，依赖独立 Relay | 服务决策已定：首版使用用户自托管 HTTPS/WSS Relay，由用户配置服务地址；Delta 自有实现尚未开始。首版默认只读，分享链接限时并可撤销 | 部署与连接 Relay、创建/打开/撤销分享、到期拒绝、只读授权、断线和 Relay 故障 |

CUA、浏览器、Forge 和 Worktree 已有 Delta 自有模块；模块存在不代表产品已可用或通过验收。新引入的 Playwright 依赖及 Windows PowerShell/Win32 适配仍须核查许可、安全、维护状态、平台兼容与发行路径。LiveShare 已选自托管 Relay，但实现、身份凭据发放、访问协议和真实部署验收尚未完成；用户自选 Relay 会接触经其转发的会话数据，必须在分享前清楚说明。DeepSeek 官方登录非阻断首发不豁免 LiveShare 的这些前提。

## 约束和执行顺序

1. 列出各功能首版最小行为、用户授权与第三方前提，并映射到非实验来源或 Delta 自研实现。
2. 审核并裁剪上游实验功能及专属装配；不能仅因名字相似删除核心依赖，也不能通过重命名实验代码伪装自研。
3. 在 Delta 仓库外完成替代实现、裁剪、Delta 化和预检，再首次导入已处理完成的候选源码。
4. 对实际导入的最终提交分别验证五项功能、已排除的上游实现和既有核心功能，再执行中央门禁。

这项澄清使原先“首版无需大量自研”的估计不再可靠。LiveShare 的 Relay 部署方式已由用户选定，但身份凭据、只读授权边界和真实部署验收仍须落实；Windows CUA 及浏览器的实机兼容性也待核实。若某项无法形成可用实现，应将首版标为未完成，不得擅自删除功能或把失败说成通过。

## Forge 本轮验证

Delta 自有只读适配器参考 [GitHub Pull Request REST API](https://docs.github.com/en/rest/pulls/pulls) 与 [GitLab Merge Requests API](https://docs.gitlab.com/api/merge_requests/)。对 GitHub `cli/cli` 的第 1 个 Pull Request、GitLab `gitlab-org/gitlab` 的第 1 个 Merge Request 分别执行了匿名真实读取，返回状态分别为 `closed` 和 `merged`。已以模拟服务验证凭据读取、令牌轮换和会话日志脱敏，并验证本地插件包可加载；未验证真实私有仓库凭据、自建 GitLab、桌面界面或最终发行包，尚不能认定 Forge 首版能力完成。
