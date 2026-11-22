# Delta 开发指令

Delta 是基于 DeepSeek Harness 运行时持续维护的 AI 工作应用。当前产品基座是 Electron、Node.js 与 Cordis；不得把规划中的模块或旧版 Tauri 架构写成已实现功能。产品边界见 [Delta 架构](governance/ARCHITECTURE.md)，源码归属见 [仓库说明](governance/REPOSITORY.md)，文档导航见 [Delta 文档中心](docs/README.md)。

## 权威与执行范围

- 跨仓库工程规则以 [Action Worker 中央治理](https://github.com/fongap-labs/action-worker/blob/main/docs/01_SHARED_GOVERNANCE.md)、机器策略及 [Delta 专属治理入口](governance/GOVERNANCE.md) 为准。冲突时遵循中央机器合同、中央共用规范、Delta 产品边界、具体实现文档的顺序，不复制或弱化中央门禁。
- 产品源码、依赖锁文件、编译参数和不可外置的产品约束归 Delta；共享构建、可外置测试、CI、合并与发布由 Action Worker 维护。未经中央 [工程归属策略](https://github.com/fongap-labs/action-worker/blob/main/policies/engineering-ownership.json) 批准，不得新增本地测试源码、CI 执行器或工作流；新增可外置回归测试应放入 [中央 Delta 测试包](https://github.com/fongap-labs/action-worker/tree/main/tests/packs/delta)。
- 不直接写入 `main`。PR 需当前提交的 `PR Governance`、`CI Evidence` 和 `validate-merge` 全部签名成功，由中央合法合并；禁止降门禁、空跑测试、伪造报告或自行签发通过状态。
- 提交标题遵守中央 `feat`、`fix`、`chore` 类型和英文格式；只维护根目录一份中文 [更新日志](CHANGELOG.md) 的 `Unreleased` 区域。
- 根目录 `CLAUDE.md` 是 `AGENTS.md` 的符号链接；维护本文件，不复制第二套指令。子目录规则只补充本模块约束，不重复全局规则。

<a id="repository-layout"></a>
## 仓库入口

- `apps/` 为桌面、Web 与命令行应用；`packages/` 为共享运行时和插件模块，细节见 [包目录](packages/README.md) 与 [包开发约束](packages/AGENTS.md)。
- `vendor/` 是固定版本的 Cordis 源码；`python/` 为 Python SDK；`native/` 为原生模块，修改前查阅各自 README。
- `resources/brand/` 是 Delta 品牌资源的正式来源；`packaging/` 为产品专属打包描述。
- `docs/` 保存正式文档，写作规范见 [中文文档维护说明](docs/AGENTS.md)；`.agents/notes/` 保存可追溯的设计决策，`.agents/skills/` 保存按需调用的工作流程；`website/` 负责现有文档投射，不另建正文副本。
- `benchmarks/` 与其他旧版测试属于待收敛存量；在中央建立等价覆盖、真实双跑并核验后，才可以清理，不能直接搬入新的 `tests/`。

<a id="pre-stable-apis-and-released-session-data"></a>
## API、Session 与兼容

- 现有公开 API 仍处于预稳定阶段：修改接口要同步更新全部实际消费者。已发布 Session 格式必须可读取，不能移动、覆盖或删除已提交的格式代际；相邻版本迁移不意味着自动支持降级。SQLite 的 `SCHEMA_VERSION` 只能单调递增，详见 [Session 版本状态](docs/session-format-status.md)、[持久化类型变更检查](docs/cookbook/reviewing-persistence-type-changes.md)。
- 影响外部用户的破坏性变更立即补充 [升级指导](.agents/skills/dsh-create-upgrade-guide/SKILL.md)，不能只留在 PR 描述或提交记录中。
- `dsh`、Cordis、既有上游包名和协议字段仍承担运行兼容用途，不做无收益的全局重命名；Delta 自研模块使用 Delta 标识。
- 对模型可见的输入与结果必须能从 Session 事件重建；增加模型可见数据时同步定义持久事件。修改 Agent 循环、Session 生命周期或 `SessionEventMap` 时检查 TypeScript 与 Python SDK 的消费者及预期输出；[Session 投影流程](.agents/skills/dsh-session-projection/SKILL.md) 按需调用。

<a id="conventions"></a>
## 核心开发约束

- 代码采用严格 TypeScript 与 ESM；同包内相对导入按现有约定使用 `.ts` 扩展名，跨包使用包名。源码检查与构建后 `lib/` 执行环境须分离；Host、Client 使用各自编译入口，详见 [开发文档](docs/development.md)。
- 插件应通过既有 Cordis 扩展点实现新能力，不能任意改动 Agent 主循环。注册必须由 `ctx.effect()` 或 `ctx.on()` 管理并可释放；Waterfall 监听器要调用 `next()` 才能继续委托，见 [Cordis 机制](docs/cordis-primer.md)。
- 一个能力接口应明确 Service Definition、Service Provider、Consumer 各自职责；跨边界 ID 使用品牌类型，类型明确的同进程值不做无意义的运行时校验。解析、持久化、进程、工具 JSON、网络与权限入口应做必要验证；禁止新增 `as unknown` 转型，详见 [类型规则](.agents/notes/implemented/process/2026-09-19-no-unknown-casts.md)。
- 配置默认值由所属模块显式解析，部署可变项必须进入可验证的 `Config`，而非隐藏硬编码。参数错误应尽早、明确失败，不得静默跳过依赖缺失。异常使用结构化错误、可行动的本地化提示和明确的处理归属；详见 [错误处理](.agents/skills/dsh-error-handling/SKILL.md)。
- 类型事件保持声明合并；服务方法、事件载荷及不显然的导出契约使用必要 JSDoc。分支处理封闭联合时覆盖所有判别类型；生命周期、并发、清理和资源所有权变更先阅读 [防御性工程参考](docs/defensive-patterns/README.md)。
- 应用界面和插件面向用户的文案保留中文、英文运行时语言资源，走既有类型化词典；说明文档统一逐步收敛为简体中文。界面样式及交互修改按 [UI 设计技能](.agents/skills/dsh-client-ui-ux/SKILL.md) 检查。
- 仅为长期有效的架构取舍创建 Agent Note，机械调整无需新增；冻结归档不可改写或当作当前规范。注释讲清行为、时序、失败及安全约束，不记录冗余的调试和推理过程。

<a id="commands"></a>
## 常用验证入口

```sh
pnpm install --frozen-lockfile
pnpm run build:artifacts
pnpm run build:check
pnpm run typecheck
pnpm run test
pnpm run test:coverage
pnpm run doc-sync
pnpm run test:docs
pnpm run website:build
```

命令只是现有仓库开发入口，不是新建本地 CI 的授权；最终证据由 Action Worker 的受信任 Linux、Windows 和安全检查签发。按改动范围选择必要的本地验证，不能把未执行的检查报告为成功；需要构建产物的验证先运行 `build:artifacts`，再运行 `build:check`。真实模型测试与快照检查规则见 [测试说明](docs/testing.md)；产品可见的非平凡行为应有相应回归证据，禁止用空测试替代。

<a id="host-sandbox-failures"></a>
遇到沙箱拒绝凭据、网络、IPC、进程或文件访问时，保留失败证据，在授权边界内尝试所需的最小权限重试；不得为了得到绿色结果跳过检测、解除权限控制或绕过产品沙箱。

<a id="run-relevant-checks-locally"></a>
推送前按 [现行检查流程](.agents/skills/dsh-pre-push-checks/SKILL.md) 选择与改动相关的验证；跨平台全面验收由中央 CI 负责。

<a id="secrets--env"></a>
## 凭据和发布

不得提交密钥。真实模型测试按 [测试规范](docs/testing.md) 使用 `DEEPSEEK_API_KEY` 等已有配置，Windows 发行与签名遵守 [现有打包指引](apps/desktop/windows-signing/README.md)。`cordis.yml` 仅在约定的 `config` 与 `disabled` 位置允许 `!!js`，其他元数据保持字面值，见 [配置说明](docs/cordis-primer.md)。

<a id="defensive-patterns"></a>
## 安全与防御

修改远端服务、客户端状态、会话、进程或生命周期时，阅读 [防御模式](docs/defensive-patterns/README.md)。权限判断不能仅依赖方便调用的共享函数；涉及模型、插件、系统命令及外部工具的敏感操作须按真实权限边界验证。

<a id="type-safety-and-documentation"></a>
## 文档与类型

代码变更同步更新所属 README、JSDoc、技术参考和必要的生成文档。Markdown 使用中文正文与原样技术标识，遵守 [文档维护规范](docs/AGENTS.md)；旧版双语三件套仍按既有配对检查迁移，不擅自关闭检查或删除格式兼容资料。源码导出使用明确的 TypeScript 类型与必要契约说明；代码注释避免复述实现。

<a id="editing-these-instructions"></a>
## 指令维护

本文件只保留每次开发必须遵守的全局约束；特定模块细节留在所属 README 或 `AGENTS.md`。改变规则前确认中央与本地权威，按实际影响更新引用。旧文档的外部锚点保留在相关章节，避免历史链接失效。

<a id="vendoring-policy"></a>
## 固定依赖

`vendor/` 依赖依据 [固定来源清单](vendor/README.md) 更新，变更时复核上游版本、Delta 本地修补及实际构建兼容性；不以自动同步覆盖当前产品主线。
