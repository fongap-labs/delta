# Delta Forge 基础模块

Delta 自有的只读 GitHub / GitLab 变更请求适配器，默认关闭。用户启用后可使用 `/forge github owner/project 123` 或 `/forge gitlab group/project 123`；GitLab 支持多级项目路径。官方服务凭据分别读取 `DELTA_GITHUB_TOKEN`、`DELTA_GITLAB_TOKEN`，每次操作重新通过凭据服务解析，未配置时可读取公共资源。请求内容中可能含私有项目信息，会保存在会话记录；命令参数和令牌不会写入该记录。

Agent 可在用户任务需要时调用 `forge_read_change_request`。每次网络读取都先由应用审批界面显示具体服务、仓库和编号；只有一次性批准才会继续。拒绝、取消或没有可用审批通道均不会联网。远端标题和描述会以不可信外部数据标识后提供给 Agent。

自建 HTTPS 服务由用户在插件配置中明确设置 `github.origin` 或 `gitlab.origin`，并以同一配置的 `tokenRef` 显式指定该服务的凭据引用；不指定则不发送令牌。凭据不放在地址中，重定向不会被自动跟随。接口响应有大小、字段长度和时间上限。实际便携包、自建服务及真实私有账号尚未验收。
