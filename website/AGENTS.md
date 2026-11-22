# 文档网站维护规范

本目录仅维护 VitePress 配置、展示资源和发布清单；正式说明文本以 [Delta 文档中心](../docs/README.md) 为准。遵循[根目录指令](../AGENTS.md)和[文档规范](../docs/AGENTS.md)。不要在 `website/` 新建重复的中文、英文、API 或产品说明目录；本文件是本目录唯一需要长期维护的 Markdown。

## 内容投射与构建

- [发布清单](docs.ts)定义权威源文档到站点路由的映射。存量双语路由与中文单版本化必须分批调整，不能靠复制 Markdown 文件或关闭翻译校验完成迁移。
- 投射器将可重建的 Markdown 写入被忽略的 `website/.generated/`。禁止编辑或提交 `.generated/`、`.cache/` 和 `.dist/`。
- VitePress 解析配置后，生产构建会清理配置的输出目录；清理必须拒绝逃逸真实站点根目录的路径，不得沿符号链接删除其目标。原始 Markdown 的输出也不得覆盖构建已生成的文件。
- 构建会为路由生成原始 Markdown 镜像（索引路由包含父级别名）及根目录的 `llms.txt`；这些都是运行时产物，不是源文件。页面链接去掉末尾斜杠并添加 `.md` 可访问原始文本。
- 修改本目录后按现有命令运行 `pnpm docs:check`。此检查禁止增加未被忽略的 Markdown；中央 CI 仍决定最终合并结果。

## 页面交互与资源

- Markdown 复制与查看操作使用投射器的 `rawMarkdownPath` 和站点 base，完整处理 `index.md` 路由。复制读取使用 `?dsh-raw=1` 协议；开发环境的脚本导入仍交给 Vite。剪贴板行为必须位于用户触发的手势和页面生命周期内。参见[页面 Markdown 操作](.vitepress/theme/page-markdown-actions.ts)。
- Mermaid 图与单独成段的正文图片共用全屏查看器和缩放控制；增强只作用于可访问的原始元素，图片应加载成功且具有非空 alt。链接图片、行内图片、装饰图片和带 `data-no-zoom` 的图片保留原交互；100% 控制对应原始尺寸。路由、语言、主题或源变动时关闭查看器，销毁时释放观察器、监听器和滚动锁。参见[查看器](.vitepress/theme/mermaid-viewer.ts)。
- 原生代码分组使用隔离的表单单选控件，避免复制搜索片段时清除平台选择。搜索片段和多页应用构建无法切换标签时须显示全部命令块；详见[代码分组适配器](.vitepress/code-groups.ts)与[对应测试](tests/code-groups.spec.ts)。
- 至少包含十条数据行的长表格支持独立文本过滤与随页面滚动的表头。匹配基于可见单元格文本，忽略大小写及查询前后空白，保留原有行顺序与链接；清空过滤后恢复全部行。小表格不改变。过滤逻辑归属[主题适配器](.vitepress/theme/table-filter.ts)，保留横向滚动与完整原始 Markdown；水合后插入控件时须兼容初次锚点导航。详见[对应测试](tests/table-filter.spec.ts)。

以上仅约束当前网站实现，不创建 Delta 独立 CI、测试基础设施或发布门禁。
