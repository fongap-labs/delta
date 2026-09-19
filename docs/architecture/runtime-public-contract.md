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
- resume 保持原 `run_id`；
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