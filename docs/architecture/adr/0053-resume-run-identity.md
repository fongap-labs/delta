# ADR-0053：恢复运行沿用哪个运行身份

状态：**已接受**（所有者，2026-10-10）。运行时已实现；桌面端的「Resume」动作为独立变更，其落地记录见 CHANGELOG。

## 背景

运行时公共合同（`runtime-public-contract.md`，第 2 节「Run」）规定「resume 保持原 `run_id`」，不变量 2 规定同一运行身份贯穿 Ledger、Artifact、Validation、SideEffect 与 Source。以下为撰写本 ADR 时的代码状态（已核对，并非假设）：

1. `RuntimeHandle::resume`（`crates/delta-core/src/runtime/handle.rs`）生成一个**新的** `run_id`（`uuid_v4()`），交给 `RuntimeHost::set_run_id`；`RuntimeHost::resume` 在该新 id 下写入 `{"kind": "resume"}` 的 `run.started`。被中断的运行不受影响。`retry` 同样如此。
2. Ledger 状态机已经允许原地恢复：`Ledger::transition` 接受从 `running`、`resumed` 与 `interrupted` 到 `run.resumed` 的转换（`ledger.rs`，ADR-042）；`open_runs` 把 `resumed` 视为开放。当时没有任何代码产生 `run.resumed`。
3. 检查点同时携带两个 id（`CheckpointRecord` 含 `run_id` 与 `session_id`）；`CheckpointReader::list(None, Some(session_id))` 返回某个会话的检查点，因此运行时无需询问界面，即可找到会话最后处理的运行。
4. `recover_interrupted_runs` 在重启时，以 `run.interrupted` 关闭执行中途的运行，并保持停在人工操作上的运行为开放状态。

因此合同与代码不一致。这种不一致不仅体现在 Ledger 上：产出物、验证记录、副作用状态、检查点以及新的 `source.read` 记录（PR #138）都以 `run_id` 为键。恢复之后，它们被分散在两个 id 下，因此任何读取"这次运行"的功能（信任条、审计导出）只能看到恢复之后的部分，而被中断的部分看起来像是一次没有结果就结束的运行。

因此，在撰写本 ADR 时，桌面端在「Interrupted」之后不提供「Resume」动作：状态会显示，但用户只能发送新消息。

## 方案

**A. 保留新的 `run_id`，并修改合同。** 恢复变为"延续会话的一次新运行"。
- 优点：无需改代码；恢复与重试保持一致。
- 缺点：不变量 2 被削弱（同一项逻辑工作有两个身份）；每个想要"整个运行"的消费者都必须跟随一个尚不存在的链接；被中断的运行永远以"中断"关闭。

**B. 沿用原 `run_id` 恢复，并写入 `run.resumed`（推荐）。**
- 优点：符合合同与不变量 2；一次运行保持同一组事件、产出物、验证、来源与检查点；状态机已允许；`recover_interrupted_runs` 与 `open_runs` 已把 `resumed` 视为开放。
- 缺点：一次运行可能有多个 `run.started` / `run.resumed` 段，因此"时长"和"最新事件"的消费者不能假设只有一段；"已关闭的运行中，中断的运行不再列出"这一规则需要显式的 `resumed` 事件（它已经有了）；运行时必须自行选择要恢复的运行。

**C. 新的 `run_id`，并记录 `resumed_from: <旧 id>`。**
- 优点：已关闭的运行保持不可变；链接是显式的。
- 缺点：每个"这次运行"的读取者都必须遍历链条；产出物与来源的拆分与现在相同，只是有了文档记录；两个 id 仍然描述同一项工作。

## 决定（提议）：B

`resume` 沿用原运行身份。

- **选择运行。** 由运行时而非界面选择：会话最新检查点所属的运行，且仅当 Ledger 显示该运行最新的生命周期状态为 `interrupted` 时。若不存在这样的运行，或会话已有活动运行，则返回错误。绝不静默地改为启动新运行。
- **事件。** 通过 `Ledger::transition`，向该运行追加 `run.resumed`（actor 为 `user`，payload 为 `{"kind": "resume", "previous_status": "interrupted"}`）。然后像现在一样，从会话消息继续循环。`run.started` 在每次运行中只写入一次。
- **返回值。** `runtime_resume` 返回与被中断运行相同的 `runId`，这正是桌面端已有的预期。
- **副作用。** 被中断运行的工具调用保持其状态。处于 `Uncertain` 的调用仍然是不确定的，且不会被重放（不变量 4）。恢复的段会发出新的工具调用 id；幂等日志以 `(run_id, tool_call_id)` 为键。
- **`retry`。** 本 ADR 不改变重试。重试跟在失败的运行之后，开始一次全新的尝试，因此保持新的 `run_id`。如果它也应当链接到失败的运行，那是另一项决定（将选项 C 的 `resumed_from` 仅应用于重试）。
- **合同文本。** 第 2 节（Run）已经描述了 B 的做法；补充确切的事件、选择规则，以及"拒绝而不是新建运行"的规则。不变更不变量。

## 后果

- 代码：`handle.rs` 的恢复路径根据检查点读取器与 Ledger 状态选择运行 id；`RuntimeHost::resume` 调用 `transition("run.resumed")`，而不是写入 `run.started`；断言新 id 行为的现有测试需要修改。
- 所需测试：恢复已中断的运行会保留其 id，并且只追加一次 `run.resumed`；没有已中断的运行时，恢复会被拒绝，且不写入任何内容；运行活动时恢复会被拒绝；已关闭（completed、failed、cancelled）的运行不能恢复；哈希链在各段之间仍可验证；恢复之后写入的产出物与 `source.read` 记录位于原 id 之下；中断之前处于 `Uncertain` 的调用，在之后仍为不确定。
- 桌面端：只有此变更落地之后，界面才能在"Interrupted"旁显示"Resume"。它仅在运行时报告中断时提供，按钮调用 `runtime_resume`；界面不自行推导（不变量 6）。
- 数据：无需迁移。在此变更之前被中断的运行，同样可以按相同方式恢复，因为检查所用的数据已经存在。

## 所有者的答复（2026-10-10）

1. 接受 B。
2. 重试保持不变：新的 `run_id`，不链接失败的运行。
3. 停在人工操作（审批、提问）上的运行，不通过本命令恢复。它保持在待定的决定上，通过回答该决定继续；`resume` 会拒绝它，因为它的最后状态不是 `interrupted`。

## 未验证事项

在工具调用执行中途（状态为 `Executing`）发生中断之后的行为，是通过阅读 `recover_interrupted_runs` 与幂等代码得出的，未经复现。上面的测试列表包含该情形，以便在工作完成时加以检验。
