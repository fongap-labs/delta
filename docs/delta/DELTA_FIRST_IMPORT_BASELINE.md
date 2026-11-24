# Delta 首版实施：基线与已发现事项

> 日期：2026-11-23。本文记录首轮实施前盘点，后续执行状态见文末“当前导入状态”。

## 已固定的本次分析输入

| 对象 | 不可变提交 |
|---|---|
| Harness 候选分析版本 | `4b399c39a6d1479cd84ca591b4d7c88e24d38a96` |
| Delta 当前 main | `96d36e875dcd24a28926b9993843ddac9ed11a8f` |
| Action-Worker 当前 main | `69c53505ad4fd83fe12092986fdc49eaa9424119` |

三个源码副本已分开存放在工作区，Harness 未导入 Delta 仓库。候选 SHA 是本次盘点输入，不表示已通过产品或法务验收，也不排除实施中经明确记录后重新锁定。

## 真实源码发现

1. `apps/desktop-host/package.json` 直接依赖 `@deepseek-ai/dsh`、Cordis、账号及插件管理器。应先区分 CLI 内部依赖与用户命令，不能删除整个 CLI 包。
2. `packages/bundle/base/cordis.patch.yml` 包含会话遥测装配及收集端点；配置注释称当前默认模式仅在用户明确反馈后导出。`packages/bundle/web-app/cordis.patch.yml` 包含桌面产品遥测和产品分析装配。桌面 `main.ts` 与 `welcome-backend.ts` 也有分析相关调用。上述是裁剪工作入口，不代表扫描已经覆盖所有外联或必须机械删除相关所有代码。
3. Electron `userData` 和 Host/CLI 使用的 `DSH_HOME` 是不同路径边界；只修改其中一个不能证明完整隔离。
4. 工作区配置包含 `packages/*/*`、`apps/*`、vendor 和 native 路径；新增或移动包必须同步验证工作区、构建及动态装配。
5. 对 439 个纳入静态盘点的 package.json，以 desktop、desktop-host、CLI 和 web 为入口，递归包含 dependencies、devDependencies、optionalDependencies 与 peerDependencies，得到 381 个本地包，未发现未解析的 workspace 引用。此统计是过度包含的静态上界线索，不是批准保留清单，不覆盖动态装配、资源或安装脚本，也不能证明运行闭包完整。

## 治理与环境发现

1. Action-Worker 的 `tests/packs/delta/pack.json` 当前仍以 Python/pytest 为运行入口，不能证明新 Electron/TypeScript 产品的覆盖。
2. `scripts/central-engineering-gate.ts` 和 `scripts/engineering-gate-api.ts` 现有的一次性 Delta 导入比较规则绑定旧基线 `7556dc8dfede03053e35359f2b55dd299da1f3ee` 和旧上游 `2e39e70496aa02986ca6a820c5d497e816b69d7e`；当前 Delta main 不是该旧基线，不能假定这项规则适用于本次导入。需在中央按可信流程设计来源审查及测试归属，不能自行扩大豁免。此项尚未对候选 PR 实际运行门禁。
3. 系统环境 Node 为 `v26.13.0`、pnpm 为 `10.34.9`；候选上游 package.json 要求 Node `^24.22.3 || >=26.13.1` 与 pnpm `11.28.5`。已在独立工作目录准备 Node `v26.13.1` 和 pnpm `11.28.5` 并验证版本命令；尚未安装产品依赖、构建产品或验证中央运行器。

## 已明确的首版要求

- 首发为 Windows x64 便携版，使用 `App/`、`Data/`、`Other/` 目录。需要验证搬迁、相对路径、目录不可写、中文路径、并发和用户数据隔离。
- DeepSeek 官方账号可选，登录失败不阻断首发；不得影响不登录时的正常使用，受限情况应告知。
- Harness 核心能力继续保留，首版正式能力边界仍须落实到具体模块与测试。CLI 沿用此前首版保留要求，具体命令按实测确定。
- 用户澄清：CUA、浏览器操作、Forge、Worktree 和 LiveShare 等产品能力继续保留；不直接吸收 Harness 处于实验状态的实现。固定上游版本中这五项涉及实验性实现或提供方，需核查非实验来源或 Delta 自有替代，不能假定已有可用实现。
- 上游桌面入口在模块初始化早期即读取 Electron `userData`，随后设置日志路径；便携目录必须在相关读取前正确初始化，并同步处理 Host 的 `DSH_HOME`、子进程和 CLI；具体接入点须做运行时验证。

## 下一步工作与未决项

- 建立 Harness 核心能力、上游实验性实现与五项保留能力的来源和依赖映射，逐项确定可验证的替代路线。首次导入前必须完成既定能力及必要裁剪；如替代尚不可用，记录阻断，不通过改名上游实验代码绕过。
- 建立实际能力到 Cordis 装配、源码、资源和 native 的映射；形成可审核保留清单和明确禁止项。
- 在仓库外设计可重复处理、来源与补丁记录；先处理遥测入口、独立数据目录，再验证真实功能。
- 适配 Action-Worker 的测试包、运行器和受信任来源规则；保持现有安全、命名、合并门禁。
- 完成候选源码后，在仓库外预检；只有完成全部规定裁剪及 Delta 化的候选，才能首次导入 Delta 受控分支。

## 当前导入状态（2026-11-23）

首轮裁剪源码已导入基于 Delta 远端 `main` 提交 `3effeb139e9af5198bfa5d380b1d3457caea4fcb` 建立的工作分支 `codex/delta-v1-source-import`。导入前回放报告记录 Harness `4b399c39a6d1479cd84ca591b4d7c88e24d38a96`、334 个允许包和 59 个 Harness 实验包排除。该分支仍须经过来源复核、中央 PR 门禁和绑定最终提交的产品验收。

曾有一个更早源码版本生成的本地便携包并用于启动界面观察；它不包含当前候选的后续 Forge 等修改，不能作为当前分支的验收证据。根据用户要求，本轮不再执行本地发行打包。当前源代码的完整产品构建、CUA/浏览器/LiveShare 实现及真实验收、插件兼容、官方登录、出站流量和最终发布产物均未据此宣告通过。
