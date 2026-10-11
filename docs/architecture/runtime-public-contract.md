# Runtime Public Contract

> 本文定义 Delta 当前稳定的 Runtime / Human Control 公共语义。它描述行为契约，不绑定内部函数名或可替换实现细节。
>
> Desktop 通过 Tauri Commands / Events 直接使用进程内 Rust `RuntimeHost`。Rust Runtime 是产品执行与状态权威。
>
> 架构边界见 `target-architecture.md`。

## 1. 范围

稳定语义包括：

- Workspace / Session / Run identity 与 lifecycle；
- RunEvent / Ledger；
- SideEffect / Idempotency；
- Approval / Policy；
- Artifact / Validation；
- Source / Citation；
- Checkpoint / Recovery；
- Steer / Follow-up / Cancel；
- Runtime events / request identity / cancellation / backpressure；
- TypeScript ↔ Rust direct IPC 需要保持的用户行为。

不冻结：

- Python class / mixin 结构；
- HTTP endpoint 路径；
- Tauri command 的内部函数名；
- sqlite column 顺序；
- Worker 内部实现；
- 内部 facade。

核心原则：**实现可以演进，用户语义和 Authority 不得静默漂移。**

## 2. Core 对象

### Workspace / Session

- Workspace 是本地资源与授权边界；
- Session 是持续工作上下文；
- UI 可以展示状态，但正式事实来自 Runtime / Trust / Work authority。

### Run

- `run_id` 是一次执行的稳定 identity；
- resume 保持原 `run_id`：Runtime 自己选择被中断的 Run（该 Session 最新 checkpoint 所属、且 ledger 最新 lifecycle 状态为 `interrupted` 的 Run），写入 `run.resumed` 后继续，不新开 Run；没有可恢复的 Run 时拒绝，不静默新建；停在审批 / 提问上的 Run 通过回答该决定继续，不走 resume（ADR-0053）；
- interactive turn、automation、resume 都必须归属明确 Run；
- latest lifecycle state 决定 recoverability；
- completed / failed / skipped / cancelled / validation-failed 为 closed。

### RunEvent / Ledger

核心字段保持：

```text
run_id
type
seq
ts
actor
payload
workspace
```

长期事件类别包括：

```text
run.*
tool.*
approval.*
side_effect.*
artifact.*
validation.*
source.*
user.steer.*
```

Human Control 事件必须可复盘。

### SideEffect

核心状态：

```text
Planned → Executing → Committed | Failed | Uncertain
```

不变量：

- operation identity 稳定；
- 相同 identity + 不同 args hash → fail-closed；
- 已执行但无法确认持久化 → `Uncertain`；
- `Uncertain` 不自动 replay。

### Artifact / Validation

Worker 不能直接宣布正式 Artifact。

```text
Worker staging
  ↓
Boundary / hash / validation
  ↓
Formal Artifact registration
  ↓
Ledger
```

Validation 是完成证据之一，不能由模型自然语言“已完成”替代。

### Source / Citation

Source / Citation 记录任务实际依据。无法确认 citation range 时必须显式为 unverified；只有满足有效条件的 citation 才计入 fully-valid。

#### Run 读取过的来源（`source.read`）

Run 读取过的来源是 Runtime 自己确认的事实：该 Run 内某次 tool 调用读取了某个来源的某个版本。它回答“这次运行看过什么”，不回答“答案依据了什么”。

事件：

- `source.read` 写入**该 Run 自己的** ledger 流（`run_id` 为实际 Run），与 `tool.completed` 一样是非终态事件，不影响 `open_runs` / `run_status` 对 lifecycle 的判断；
- `source.registered` / `citation.marked` 继续写入 `$source` 命名空间，语义不变。`source.read` 通过 `source_id` 引用其中的 SourceRef；来源 identity、fingerprint 与 `current` / `changed` 状态仍只由 Source authority 维护。

payload：

```text
source_id     SourceRef.id
origin        与 SourceRef 相同
location      与 SourceRef 相同
fingerprint   读取当时的内容指纹
tool_call_id  触发读取的 tool call
tool          tool 名称
read_at       RFC 3339 时间
```

产生规则：

1. 只由 Runtime 在 tool 执行边界产生。Worker / Capability 不能自行声明“我读过 X”；
2. 仅当 capability metadata 的 `category` 为 `read`，且 Runtime 在执行前自己核对过输入（文件的路径与 SHA-256；网络读取则为 URL 与响应哈希）时才产生。Runtime 无法核对的读取不产生记录，而不是产生一条未经确认的记录；
3. 仅在 tool 成功完成后产生；失败、被拒绝、被取消的调用不产生；
4. 同一 `tool_call_id` 对同一来源只记一次，幂等重放不重复；
5. 记录写入失败时该 tool 调用按失败处理，与 `tool.completed` 写入失败一致，不静默丢失；
6. 首个实现范围只含工作区文件读取（`file.read`）。网络与 connector 读取要先让 Runtime 能核对响应，之后沿用同一事件，不再改契约。

不保证：

- 不表示答案内容来自该来源；
- 不表示存在任何 citation，或 citation 有效；
- 不表示来源此后没有变化（用 SourceRef 的 `status` 判断）。

读取（供 UI 与导出）：

按 `run_id` 返回该 Run 的来源列表，每项含 `source_id`、`origin`、`location`、`fingerprint`、`read_at` 和 `cited`。`cited` 表示该 Run 内是否有指向该来源、且通过 `validate_citation` 的 `citation.marked`；在 citation 落地之前恒为 false。这是只读查询，UI 不得自行从 tool 输出推导来源（不变量 6）。

展示约束：

- 文案只说“本次运行读取过”。只有 `cited` 为 true 才可以说“引用”；任何情况下都不得说“依据”“来源于”；
- 展示 `location` 时，文件使用相对工作区的路径，URL 去掉 query 与 fragment。

兼容与扩展：

- 只新增事件类型与只读查询，不改既有事件 payload 与 SourceRef；没有 `source.read` 的旧 Run 返回空列表，UI 不显示；
- 为 citation 预留：`citation.marked` 增加可选字段 `run_id` 与 provider 报告的位置，`cited` 由此推出。provider-native citation 必须对应已登记的 SourceRef 并通过 `validate_citation` 才计入，否则保持 unverified。该扩展另起 PR；若它改变“谁有权宣布 citation 有效”，需要 ADR；
- 本变更不需要 ADR：属于新增事件类型与只读查询，不改变 lifecycle / approval / side-effect / steering 语义，不改变 identity / persistence 语义，不转移 Authority（见第 7 节）。

### Checkpoint / Recovery

恢复必须保留 run identity、phase、pending human decision、last committed event、recent artifacts、uncertain side effects 和 error context 等关键事实。

## 3. Human Control

### Steer

- 修改当前 Run 方向；
- 保持同一 `run_id`；
- 在 safe point 应用；
- 必须形成审计事件；
- 无法安全应用时 deferred / rejected，而不是静默丢失。

### Follow-up

- 当前 Run 完成后处理；
- 不伪装成对当前 Run 的实时修改。

### Cancel

- 请求停止当前 Run；
- 尊重 side-effect uncertainty；
- 不把已经产生的副作用伪装成未发生。

### Egress level（出境档位，ADR-0057 / ADR-0061）

- 每个工作区一个档位：`open`、`approved_endpoints`、`private_only`、`ask_each_time`；未单独设置的工作区跟随默认档位；
- 默认档位由用户在首次启动（或升级后首次启动）时选择，Delta 不预选；未选择前不发送任何内容；
- 权威数据只在 Rust：状态目录下的 `egress.json`（仅本人可读，原子写入；读不出或结构不符时一律按失败处理，且不覆盖）；
- 只有桌面端设置命令能修改档位和端点标记（`approved` / `private`）；会让内容去到以前去不了的地方的修改，必须带用户确认，否则不生效；收紧立即生效；
- 每次修改写入审计日志（`audit_events.db`，tool 为 `egress.settings`）；
- 端点分类：用户标记优先；否则解析主机地址，全部地址都在本机或私有网络才算私有，混合或无法解析都不算私有；
- 判定：`open` 放行；`ask_each_time` 每次运行确认一次；`approved_endpoints` 放行已批准、私有或标记为私有的端点；`private_only` 只放行私有或标记为私有的端点。

模型请求的执行点（ADR-0057）：

- 每次模型请求前，Runtime 按工作区档位判定端点；未选择档位、被拒绝或用户未确认时，请求不发出，Run 以 `egress` 错误结束（`run.failed`），并写入 `model.request_refused`（端点主机、模型、档位、原因，不含内容）；
- `ask_each_time` 通过正常审批路径确认，每次 Run 一次（恢复或重试的 Run 重新确认），无人值守的 Run 停在 Inbox；长期授权不适用；
- 无人值守的 Run 被拒绝时，同时在 Inbox 留一条 `notice`（每个会话、每种原因一条）；
- 判定依赖端点地址时（`private_only`、`approved_endpoints` 下的私有端点），连接固定到判定时检查过的地址，不再重新解析；
- 每次发出的请求写入 `model.request`：`iteration`、`endpoint`（主机）、`endpoint_class`、`model`、`protocol`、`level`、`bytes`（消息体大小）、`sources`（本 Run 已读来源的 `source_id` 与 SHA-256）。它不包含消息内容，也不声称某个来源的内容一定在这次请求里。

出站工具的执行点（ADR-0061）：

- 把内容送出本机的工具声明 `destination`：`fixed_public`、`any_public`、`private` 或 `local_process`。远程 MCP 服务器声明 `fixed_public` 并带上 URL，本地（stdio）MCP 服务器声明 `local_process`，网页工具和浏览器自动化声明 `any_public`。`web` 或 `mcp` 类工具未声明 `destination` 时按 `any_public` 处理；
- 工具提出调用前，Runtime 按工作区档位判定：`open` 放行；`ask_each_time` 每次调用都确认；`approved_endpoints` 放行私有目的地和已批准主机，`any_public` 每次确认，其余固定公网服务和本地进程拒绝；`private_only` 只放行私有目的地，其余拒绝；未选择档位则拒绝；
- 被拒绝的调用不执行，写入 `tool.failed`（`stage` 为 `egress`）；需要确认的调用只对这一次调用有效，长期授权和自动批准不适用；
- `tool.proposed` 带 `egress` 对象：`class`、`host`、`level`、`decision`（`allowed`、`ask`、`blocked`、`not_chosen`）；不向本机之外送出内容的工具为 `null`；
- 本地 MCP 服务器可由用户在“设置 ▸ 隐私”中标记为 `local-only`（用户的陈述，Delta 不验证）；标记会让它在 `approved_endpoints` 和 `private_only` 下运行，因此需要确认，取消标记立即生效；
- 对任何档位都会拒绝的工具，不提供给模型（`offered_to_model`），不再只在执行时拒绝；
- 按路径规则（`set_path_rule`，ADR-0061 第 3 点）：对某个路径（`web_search`、`web_fetch`、`browser`、`open_url` 或 `mcp:<服务器>`）在某个工作区（或所有工作区）给出 `allow`、`ask` 或 `deny`，替换该档位对这条路径的判定；只能经由桌面端设置命令设置；会让内容去到更多地方的规则需要确认，取消规则立即生效；`tool.proposed` 的 `egress` 对象带有 `path` 与 `override`；
- 连接器工具（`category` 为 `connector`）同样受管：未声明目的地的按 `any_public` 处理，路径为 `connector:<名称>`；Suite 连接器的目的地声明尚未完成（多数连接器调用多个主机，需要按工具列出主机）。

## 4. Transport 与 IPC

Desktop 主路径：

```text
TypeScript
   ↓
Tauri Commands / Events
   ↓
Rust RuntimeHost
```

Transport 可以替换，但 request identity、cancel、backpressure、event ordering 和 Human Control 语义必须保持一致。

## 5. 不变量

1. **一个领域一个 Authority**。
2. **一份 Run identity 贯穿 Ledger、Artifact、Validation、SideEffect、Source。**
3. **Worker 不修改核心状态。**
4. **Uncertain 不自动 replay。**
5. **Human Control 可审计。**
6. **UI 不推导 Authority。**
7. **Learning 不改变权限。**
8. **Transport 可替换，领域 contract 不静默漂移。**
9. **同一领域只保留一个执行与状态 Authority，不长期保留双实现。**

## 6. Capability / Worker Contract

专业 Office、Research、Media 和 Script 能力可以继续使用 Python / PowerShell / Shell，但必须经过 Rust-supervised Capability boundary。

Worker 不得拥有：

- Session / Run lifecycle；
- core DB authority；
- Policy / Approval；
- Secrets authority；
- Artifact formal state；
- model/provider control plane。

详细见 `capability-abi.md`。

## 7. 变更流程

普通字段扩展、event / validator / capability metadata 可通过普通 PR，但必须有测试与文档。

以下破坏性变化需要 ADR：

- lifecycle / approval / side-effect / steering 语义变化；
- identity / persistence semantics 变化；
- Authority ownership 变化；
- 安全边界变化。

同一 contract 的内部实现调整必须通过 contract test + E2E 证明行为保持。