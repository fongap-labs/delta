# Delta 源码 vendor 清单

此目录包含 Delta 固定源码快照所带的 Cordis 框架与基础库。Delta 的完整上游来源以仓库根目录 [`upstream.lock.json`](../upstream.lock.json) 中的 Harness SHA 为准；逐包源码提交号没有随裁剪副本保留，因此这里不补写未经验证的上游 SHA。

## 当前清单

| 目录 | 工作区包名 | 包版本 | 许可 |
| --- | --- | --- | --- |
| `cordis/` | `@deepseek-ai/cordis` | `4.2.0-alpha.1` | MIT |
| `cosmokit/` | `@deepseek-ai/cosmokit` | `1.10.0-alpha.1` | MIT |
| `group/` | `@deepseek-ai/cordis-plugin-group` | `1.2.0-alpha.1` | MIT |
| `include/` | `@deepseek-ai/cordis-plugin-include` | `1.2.0-alpha.1` | MIT |
| `loader/` | `@deepseek-ai/cordis-plugin-loader` | `1.2.0-alpha.1` | MIT |
| `logger-console/` | `@deepseek-ai/cordis-plugin-logger-console` | `1.2.0-alpha.1` | MIT |
| `schemastery/` | `@deepseek-ai/schemastery` | `3.20.0-alpha.1` | MIT |
| `timer/` | `@deepseek-ai/cordis-plugin-timer` | `1.3.0-alpha.1` | MIT |

包目录中的 `package.json` 与 `LICENSE` 是许可清单生成器的校验依据。新增或删除 vendor 包时同步核对工作区、依赖、许可证、构建引用和本页清单。

## 同步规则

1. 仅从经审查并固定 SHA 的 Harness 快照导入；不得直接跟随移动的 `master`。
2. 先检查上游变化、Delta 实际使用方、包导出和许可证，再确定是否吸收；禁止为了保持目录一致而整体替换。
3. 每个 Delta 对 vendor 源码的新增修改都要在提交说明中记录原因、影响和验证；不直接改写上游实验实现来绕过首版排除规则。
4. 修改后运行相关 Cordis/Profile、插件加载、构建和第三方许可证门禁，并更新本页的目录、版本和许可信息。

此清单按仓库包清单维护，不宣称固定 Harness 快照中的每个 vendor 包都已独立完成功能验收。
