# ADR-0004：能力标准执行链

状态：已接受

## 背景

R8.3 要求所有能力调用都遵循单一、可强制执行的执行链。任何能力（原生、Worker 或适配器）都不得绕过 Trust、授权（Grant）、Validation 或 Ledger。

## 决定

标准执行链如下：

```
请求能力
    ↓
Registry 查找
    ↓
清单 / 需求
    ↓
Trust / Policy 评估
    ↓
需要时进行审批
    ↓
限定范围的执行授权（Scoped Execution Grant）
    ↓
执行校验（R8.1）
    ↓
分发（Dispatch）
    ↓
原生 / Worker / 适配器
    ↓
结构化结果
    ↓
Validation 验证
    ↓
正式成果物（Formal Artifact）
    ↓
Ledger 账本
```

### 强制执行点

1. **Registry 查找**：`CapabilityRegistry::get()` 是工具分发的唯一入口。未注册的工具会被拒绝。

2. **Trust / Policy**：`RuntimeHost::execute_tool_call()` 在任何分发之前调用 `policy::evaluate()`。策略拒绝则阻止执行；需要用户确认的风险等级必须经过审批。任何代码路径都不能在未经策略检查的情况下到达 `CapabilityHost::execute()`。

3. **执行授权**：`CapabilityHost::execute()` 在分发给任何 Runner 之前，调用 `CapabilityBoundary::validate_execution_grant()`（R8.1）。无效的授权永远不会到达 Runner。`WorkerProcessRunner` 还会做一次防御性的二次检查。

4. **分发**：只有 `CapabilityHost` 实现 `ToolExecutor`。`RuntimeHost` 持有 `tool_executor` 字段，并且只在 `execute_tool_call()` 中调用它，该处是唯一的调用点。

5. **成果物正式化**：Worker 将成果物暂存到 `artifact_staging_dir`。Runtime 通过哈希、校验以及 `ArtifactRegistryWriter` 注册，将暂存成果物正式化。Worker 绝不直接注册成果物。

6. **Ledger**：每个阶段（已提议、已批准、已开始、进度、已完成 / 已失败）都持久化到 Ledger。Ledger 是审计轨迹。

### 绕过防护

- **能力不能绕过 Trust**：`CapabilityHost::execute()` 只能通过 `ToolExecutor` trait 调用，而该 trait 仅在策略评估之后由 `RuntimeHost::execute_tool_call()` 调用。
- **Worker 不能绕过授权**：`CapabilityHost` 中的 R8.1 授权校验，加上 `WorkerProcessRunner` 中的防御性检查。
- **适配器不能绕过策略**：MCP 适配器（`McpCapabilityRunner`）包装 `WorkerProcessRunner`，后者由 `CapabilityHost` 分发，而 `CapabilityHost` 受 `RuntimeHost` 中的策略门控。
- **MCP 不是 Runtime**：`McpCapabilityRunner` 是一个 `CapabilityRunner` 适配器，而不是独立的运行时。它委托给 `WorkerProcessRunner`。
- **连接器不直接创建正式成果物**：连接器产出暂存输出，由 Runtime 将其正式化。
- **Skill 不能绕过能力边界**：Skill 声明所需的能力，但不直接调用 Runner。

## 后果

- 不新建顶层的"Resolver 服务"。注册表查找是 `CapabilityHost` 的内部职责。
- 该执行链通过结构强制执行，而不是靠约定。
- 新增能力类型（原生、Worker、适配器）需要实现 `CapabilityRunner`；它由 `CapabilityHost` 分发，并受完整执行链门控。
