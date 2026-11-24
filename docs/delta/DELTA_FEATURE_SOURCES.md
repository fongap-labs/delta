# Delta 五项产品能力与实现来源（工作稿）

> 用户澄清：CUA、浏览器操作、Forge、Worktree、LiveShare 产品能力保留；不直接吸收 Harness 处于实验状态的功能实现。此前将五项产品功能全部排除的记录无效。本文件是源码盘点与替代路线，不代表实现或验收已经完成。

固定分析版本：Harness `4b399c39a6d1479cd84ca591b4d7c88e24d38a96`。

| 功能 | 上游固定版本发现 | 实施调查方向 | 验收重点 |
|---|---|---|---|
| CUA | `packages/experimental/cua-bundle/`、`cua-windows/` 等属于实验组；Windows 辅助程序及权限链须考虑 | 调查适合 Windows x64 的非实验来源，或由 Delta 自研桌面操作模块 | 独立启动、授权、窗口观察、键鼠操作、拒绝和取消 |
| 浏览器操作 | 共享服务在 `packages/browser-use/`，但上游说明所列提供方为实验性包 | 核验共享服务状态、依赖和协议；评估非实验提供方或 Delta 自研适配 | 真实打开、读取、操作网页和会话资源清理 |
| Forge | 上游 Forge、GitHub/GitLab 提供方和面板在实验组 | 已编写 Delta 自有的只读 API 适配基础；GitHub 和 GitLab 官方公共变更请求读取通过真实请求验证，已接入凭据服务、命令与构建；真实私有账号和桌面验收待完成 | GitHub 和 GitLab 的认证、变更请求读取及错误处理 |
| Worktree | 上游工作树工具及其 Forge 组合属于实验组 | 评估基于 Git 的 Delta 自有工作树模块 | 创建、进入、退出、任务隔离、并发与安全清理 |
| LiveShare | 上游 LiveShare owner 和协议在实验组，依赖独立 Relay | 定义分享协议、服务部署、身份、访问范围及撤销；评估非实验来源或 Delta 自研 | 真实建立分享、访问、拒绝、权限收窄、撤销和服务失败 |

上表的调查方向不是已经选定的产品、许可或可行性承诺。不同来源也需要核查许可证、安全、维护状态、Windows 兼容与便携性。LiveShare 的服务需求不能因为 DeepSeek 官方登录非阻断就默认豁免。

## 约束和执行顺序

1. 列出各功能首版最小行为、用户授权与第三方前提，并映射到非实验来源或 Delta 自研实现。
2. 审核并裁剪上游实验功能及专属装配；不能仅因名字相似删除核心依赖，也不能通过重命名实验代码伪装自研。
3. 在 Delta 仓库外完成替代实现、裁剪、Delta 化和预检，再首次导入已处理完成的候选源码。
4. 对实际导入的最终提交分别验证五项功能、已排除的上游实现和既有核心功能，再执行中央门禁。

这项澄清使原先“首版无需大量自研”的估计不再可靠，尤其 LiveShare 和 Windows CUA 的工作量及外部依赖尚未核实。若某项无法形成可用实现，应将首版标为未完成，不得擅自删除功能或把失败说成通过。

## Forge 本轮验证

Delta 自有只读适配器参考 [GitHub Pull Request REST API](https://docs.github.com/en/rest/pulls/pulls) 与 [GitLab Merge Requests API](https://docs.gitlab.com/api/merge_requests/)。对 GitHub `cli/cli` 的第 1 个 Pull Request、GitLab `gitlab-org/gitlab` 的第 1 个 Merge Request 分别执行了匿名真实读取，返回状态分别为 `closed` 和 `merged`。已以模拟服务验证凭据读取、令牌轮换和会话日志脱敏，并验证本地插件包可加载；未验证真实私有仓库凭据、自建 GitLab、桌面界面或最终发行包，尚不能认定 Forge 首版能力完成。
