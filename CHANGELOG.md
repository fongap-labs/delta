# 更新日志

## [Unreleased]

- 维护: 精简上游 Runner 与 Windows 旧工作流测试，保留实际 Node 工具缓存隔离及 Windows PowerShell 优先级只读检查，移除失去调用者的旧归档测试辅助模块。

- 维护: 删除仅依赖已移除 GitHub Actions 工作流的桌面发布、Cloudflare 预览及 macOS 打包测试文件；实际 CI 桥接及执行清单持续由 Action Worker 可信测试包检查。

- 维护: 将上游遗留的 CI 工作流测试由约 1,900 行精简为当前仍有效的 GitLab、bubblewrap 和 Git Hook 六项检查；正式 Actions 桥接合同由 Action Worker 可信测试包维护。

- 维护: 补充 Delta Code 私有插件离线 .tgz 制品的 Host/Client、Cordis、语言资源、依赖协议及私有文件排除验收说明；不发布插件或改变默认装载。

- 维护: 明确 Delta Code 仅中英文资源、默认关闭高权限操作、缺失宿主进程服务时拒绝本机回退，以及 GitHub 明文凭据与跨账号绑定的中央运行时验收。

- 维护: 归档上游品牌规范、移动基准文档，并移除已失效的本地仓库治理测试命令和门禁调用。

- 维护: 将 Delta 品牌母版迁移至 resources/brand，更新资源引用、验收脚本和项目说明，保持图标内容不变。

- 维护: 根目录仅保留中文 README.md，移除旧版 Harness 中文介绍及对应的双语记录，保留其他文档双语校验。

- 维护: 记录 Delta Code 私有工作区包经真实 pnpm 离线 link 安装与移除的隔离 Profile 验收范围，以及尚未覆盖的注册表发行、桌面进程和 Windows 验收。

- 维护: 补充 Delta Code 临时隔离 Profile 中 Bundle 选择、停用、启用和取消选择的验收范围，继续区分模拟包解析与真实 pnpm 安装卸载。

- 维护: 明确 Delta Code 真实 Cordis 工具注册/释放生命周期冒烟的覆盖范围，以及尚未完成的 Profile 安装卸载和 Windows 发行验收。

- 修复: 为已有 Host/Client 导出清单补全 Delta Code 与共享包的运行时打包步骤，确保声明的 `lib/index.js` 和 `lib/client.js` 真正生成；中央 CI 验证构建和模块导入，不引入旧插件兼容或默认安装。

- 修复: Delta Code 只接受当前插件包名与注册 ID，删除旧工作树目录查找和移除兼容回退；不自动迁移或删除用户历史数据。

- 修复: Delta Code 内部插件 ID、设置表单、日志、凭据默认前缀及工作树目录统一采用 delta-code，并保留旧工作树的安全移除兼容。

- 新增: 将 Delta Code 源码迁入主程序与第一方插件共用仓库，调整独立插件包名和共享模块依赖，保留 Harness 装载协议。

- 维护: 将 Delta 官方 Logo 母版无损迁至 branding 根目录，更新品牌清单、欢迎界面注释与一致性校验。

- 维护: 完成一次性受审计的 Delta main 历史迁移；保留全部 Delta 代码和旧主线快照，现有 main 已继承 Harness 固定基线（迁移时 Behind 0 / Ahead 8），并通过 Windows 打包与保护规则恢复验收。

- 修复: 修正打包后运行检查仍启动 DeepSeek Harness.exe 的路径错误，统一 Delta.exe 与 macOS Delta.app 命名，防止 Windows 便携 ZIP 构建在启动验证阶段失败。

- 维护: 确立当前 main 作为 Delta 开发基线，冻结 v0.0.1 旧版分支，并明确与 Harness 独立提交历史的比较规则。

- 修复: 将 Delta Windows 便携版发布清单迁移到 Action Workers 标准目录，补齐构建字段与 Node 版本配置，并支持隔离凭据的离线打包。

- 修复: 为 Delta Windows x64 便携版发布补齐中央规范的产品清单、Node 版本与 SBOM 字段，隔离未签名离线测试打包与正式签名及更新策略，保留产物和发布门禁。

- 修复: 清理隐私采集与旧语言移除后遗留的测试引用，验证付费翻译请求不携带 Session 历史，并恢复工作区类型检查。

- 修复: 桌面诊断隐私测试的合成邮箱改为运行时构造，保留邮箱脱敏验收并满足中央源码隐私检查。

- 修复: 规范 Delta 首次导入的 4 个构建脚本名称及调用路径，同时修复 Windows 便携包验证阶段重复断裂的 PowerShell 语法。

- 新增: 导入固定版本的 DeepSeek Harness 作为 Delta 二次开发基线，沿用旧版 Delta 标识与欢迎界面，并支持 Windows 便携 ZIP 发布结构。
- 新增: 将客户端可选语言收敛为简体中文和英文，桌面应用继续使用 Electron 和兼容 Harness 的插件接口。
- 修复: 左侧边栏顶部不再显示上游 Harness 的 Logo 和文字，保留导航与窗口控制。
- 新增: 默认模型设置只允许手动添加自定义 API 地址、协议与模型，不再预装第三方模型提供商目录及专用模型扩展。
- 修复: 删除自动 Session 日志上传、产品分析、遥测采集与崩溃诊断的运行时模块，保留用户主动触发的 ZIP/Markdown 本地导出。
- 维护: 同步 SDK 最小配置、依赖锁文件及品牌和自定义模型的回归校验；构建与桌面运行验收仍在进行。
- 修复: 为 Delta 引导仓库补齐安全批准的 Action Worker 中央 CI 通知桥接，继续保持原有受保护分支与合并门禁。
- 维护: 固定 DeepSeek Harness 初始候选基线，并规定日常上游差异以基线分支为比较起点。
- 新增: 建立 Delta 第一方插件共享工作区，提供宿主能力校验、配置解析、生命周期、脱敏和路径辅助并同步冻结锁文件。
