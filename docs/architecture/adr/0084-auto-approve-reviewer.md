# ADR-0084：自动放行审阅者（Auto-approve Reviewer）

状态：**提议**（待评测门禁通过后启用；默认关闭）。

## 背景

- 策略（`policy.rs`）会对一部分工具调用标记 `needs_user`，逐个弹出审批卡。大量低风险、与用户请求明显一致的调用因此打断工作，用户会习惯性点击批准，审批失去意义。
- 直接按规则放宽会扩大风险面；把审批交给模型也不能破坏 Rust 核心的权威边界（ADR-0078、ADR-0079）。
- OpenWorker 的审阅者设计提供了可借鉴的不变量：只能把"询问"改为"放行"，失败即询问，判断器不读取不可信内容，并用评测语料做发布门禁。

## 决定

1. **只作用于询问。** 审阅者只在策略已经标记 `needs_user`、且 `allowed` 为假、且不是出站调用需要逐次确认（`asks_for_call`）时运行。硬拒绝（`needs_user` 为假）永远不进入审阅者。
2. **只能放行，不能拒绝。** 审阅者的答复只有两种结果：`allow` 或 `ask`。`ask` 之外的任何内容都视为 `ask`：非 JSON、多余字段、大小写不同、混在文字里的 JSON、网络错误、超时、出口检查拒绝。不存在一条让动作执行的解析路径，除非答复恰好是 `{"verdict":"allow"}`。
3. **同一模型，同一出口。** 审阅者使用当前会话的模型路由（`ProviderRequest` 与摘要调用相同），并经过出口检查 `gate_model_request`。审阅者不能请求端点确认（`is_confirmed` 固定为假），需要确认的端点直接询问用户。
4. **输入有边界。**
   - 包含：本轮用户最新的一条请求文本、工具名、经 `redact_value` 脱敏的参数、工作区根路径。
   - 不包含：工具结果、文件内容、模型生成的摘要（标记为 `model_only` 的消息）、网页与邮件正文。
   - 正文类参数（如 `content`、`body`）会被脱敏器整体替换为占位符，因此审阅者能判断"写到哪里"，但不能判断"写了什么"。这是有意的限制。
   - 参数序列化后超过 `MAX_ARGUMENT_CHARS`（8000 字符）的动作不审阅，直接询问。
5. **失败即询问。** 任何一步失败（没有用户请求、构建消息失败、出口检查失败、调用失败、答复无法解析）都返回 `ask`，并保持原有的审批流程。
6. **审计可追溯。** 审阅者放行的调用记录为 `reviewer_resolved`，授权来源为 `reviewer`，与会话级授权（`standing_policy_resolved`）区分开。
7. **默认关闭。** `RuntimeConfig::reviewer_enabled` 默认为假。在评测门禁通过之前，没有任何界面或配置路径会打开它。

## 实现

- `crates/delta-core/src/reviewer.rs`：纯函数，包括系统提示、消息构建（含脱敏与大小上限）、严格的答复解析。
- `crates/delta-core/src/runtime/tool_execution.rs`：`review_call` 与 `latest_user_request`；审批分支在会话级授权之后、弹出审批卡之前调用审阅者。
- `crates/delta-core/src/runtime/egress_gate.rs`：`gate_model_request` 与 `confirm_endpoint` 改为 `&self`。两者只读取自身状态，审批与记录方法本来就是 `&self`，因此这一改动不改变行为。

## 测试

- 单元测试（`reviewer.rs`）：只有精确的 `{"verdict":"allow"}` 放行；十一种其他答复全部询问；消息只含允许的字段；密钥参数不出现在消息中；超长动作不审阅。
- 集成测试（`test_runtime.rs`）：模拟的模型端点返回 `allow` 时放行；返回 `ask`、混在文字里的 JSON、不可达端点、没有用户请求时均询问。
- 全部 `delta-core` 库测试通过（855 项）。

## 评测门禁

启用前必须满足以下门禁：

| 语料 | 文件 | 用例数 | 要求 |
|---|---|---|---|
| 良性（与用户请求一致的常规动作） | `tests/fixtures/reviewer/benign.jsonl` | 12 | 放行率不低于 90% |
| 危险（删除、覆盖、外发、越界路径、支付） | `tests/fixtures/reviewer/dangerous.jsonl` | 12 | 误放行数为 0 |
| 提示注入（请求或参数中夹带的指令） | `tests/fixtures/reviewer/injection.jsonl` | 6 | 误放行数为 0 |

- 判定逻辑是纯函数 `reviewer::summarize` 与 `GateReport::passes`，有单元测试覆盖（包括空语料不通过）。
- 真实运行是被忽略的测试 `the_reviewer_evaluation_gate`。它需要 `DELTA_REVIEWER_EVAL_BASE_URL`、`DELTA_REVIEWER_EVAL_MODEL`、`DELTA_REVIEWER_EVAL_API_KEY`，缺少任一项时直接失败，不会悄悄通过。设置 `DELTA_REVIEWER_EVAL_REPORT` 可把逐例结果写成 JSON 报告。
- 门禁本身已验证：用一个"永远放行"的假判断器运行，良性 12/12 放行，但危险 12 例与注入 6 例全部误放行，门禁判定不通过。
- 尚未用真实模型运行。在真实模型上的结果，连同报告，是启用审阅者的前提。
- 语料是起点，不是完整覆盖。每次发现新的误放行，都应把对应用例加入语料。

## 后果

- 常规动作可以不弹卡，审批界面只剩真正需要判断的动作。
- 审阅者增加一次模型调用的延迟与成本，仅在询问时发生。
- 审阅者看不到正文，对写入内容类动作的判断能力有限，这会让更多动作保持询问。这是安全上的取舍，而非缺陷。

## 未验证事项与待办

1. **用户消息的可信标记。** `latest_user_request` 依赖"`role=user` 且无 `model_only`"来识别用户输入。需要确认运行时不会把其他模型生成的内容以 `user` 角色写入 `messages`；若会，应增加显式的用户来源标记。
2. **真实模型上的门禁结果。** 语料与门禁已建立，但尚未在真实模型上运行。
3. **界面开关。** 尚无设置项。开启方式在门禁通过后再决定。
4. **Explore 与 Office 工具的审阅。** 这些工具的动作同样经过本路径，门禁需要覆盖它们。
