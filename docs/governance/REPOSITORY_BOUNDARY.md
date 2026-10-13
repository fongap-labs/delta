# Delta 扩展边界（Extension Boundary）

版本：2.0

本文规定 Delta Foundation 与可选扩展之间的仓库和权威边界，说明依赖方向、职责划分、安装合同与硬性规则。

## 用途

Delta Foundation 是完整的、可独立运行的产品。可选扩展可以通过稳定的公开合同增加能力，但 Foundation 绝不得要求安装任何扩展实现才能完成构建、测试、启动或基线工作。

## 依赖方向

唯一支持的依赖方向是：

```text
可选扩展
       ↓
Delta 公开 SDK / 扩展 API / 合同
       ↓
Delta Foundation
```

禁止：

```text
Delta Foundation → 扩展实现
扩展 → Foundation 内部实现
```

## Foundation 职责

Foundation 负责：

- App 与 Core 运行时；
- Trust、Permission、Approval、State、Recovery 与 Ledger 的权威；
- 公开 SDK、Schema 与跨进程合同；
- CapabilityHost 与 Worker 监督；
- 通用的基线能力；
- 扩展的发现、校验与隔离。

Foundation 绝不得在 Core 权威逻辑中编码可选扩展的标识符、实现逻辑、厂商专属工作流或权益（entitlement）决策。

## 扩展职责

扩展可以提供：

- 能力与 Worker；
- 连接器（Connector）；
- Skill 与工作流；
- 可选的托管服务适配器；
- 能力相关的展示元数据。

扩展绝不得创建第二套 App、Core、Runtime、Trust 模型、持久化权威存储或审批路径。

## Runtime 安装合同

每个已安装的扩展拥有其专属目录，位于：

```text
<delta-state>/extensions/<source>/
```

其清单与其他扩展相互隔离。Foundation 独立地发现并校验这些清单。损坏的可选扩展绝不得阻止 Foundation 或无关扩展启动。

## 兼容性

扩展应面向已发布的公开合同，而非 Foundation 的 main 分支或内部实现路径。破坏性的公开合同变更需要一个带版本的兼容性边界。

## 硬性规则

1. Foundation 在未安装任何可选扩展的情况下，必须能够构建、测试和运行。
2. Foundation 绝不导入可选扩展的源码。
3. 扩展只能使用公开的 SDK / 扩展 API / 合同。
4. 每个扩展都必须可以独立安装、移除和诊断。
5. Runtime 权威始终留在 Foundation。
6. 这些规则由检查脚本强制执行：架构边界检查（`scripts/check_architecture_boundary.py`）在 CI 中运行；仓库边界检查（`scripts/check_repository_boundary.py`）通过 pre-commit 运行。
