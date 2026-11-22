# Browser Use（可选）
按需启用 Playwright MCP 浏览器操作。默认不装配到 Delta 的 Base/Web Bundle。

会话拥有独立 Chromium 浏览器和 MCP 连接；退出会话或卸载插件应释放资源。首次使用需在系统中安装匹配的 Chromium 浏览器。默认无头启动，适合工具调用；需要人工接管时必须显式配置 attach 模式及其本地调试端点。

安全边界：仅在用户主动选择此 Bundle 后运行。访问网页可能暴露敏感数据，不能自动上报会话日志、浏览器凭据或模型请求；默认关闭外部浏览器附加和其他实验性浏览器提供方。中央 CI 与 Windows/macOS 实测全部通过前不得宣称正式可用。

验证边界：Action Worker 中央 Linux CI 通过合成的本地回环网页，使用真实 Chromium 和固定版本的 Playwright MCP 对导航、输入、点击、双 MCP 实例隔离及关闭进行自动化实测。该测试不等同 Delta 桌面插件管理界面、Cordis Session 全链路或 macOS GUI 权限验收；以实际 CI 结果为准。
