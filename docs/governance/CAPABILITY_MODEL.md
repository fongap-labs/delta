# Delta 能力模型（Capability Model）

版本：1.0

状态：架构基线（Architecture Baseline）

本文是 Delta 能力模型的架构基线，说明能力的类型、生命周期、权威边界与禁止模式。

## 1. 目的

本文定义 Delta 的能力模型。

目标是：

- 保持 Core 稳定。
- 支持扩展。
- 让 Foundation 与可选扩展共用同一套架构模型。

---

## 2. 能力原则

Delta 遵循：

**Capability over Fork（能力优于分叉）**

新增功能时：

优先选择：

**Capability（能力）**

而非：

**Core 修改**

优先选择：

**Extension（扩展）**

而非：

**新子系统（New subsystem）**

---

## 3. 能力定义

Capability（能力）是可独立识别的能力单元。

Capability 定义以下内容：

- 身份（Identity）
- 用途（Purpose）
- 所需权限（Required permissions）
- 依赖（Dependencies）
- 执行模型（Execution model）
- 生命周期（Lifecycle）

Capability 不拥有系统权威。

---

## 4. 能力类型

Delta 支持以下能力类型。

### Skill（技能）

用途：

提供可复用的知识或任务逻辑。

示例：

- 写作辅助
- 分析工作流
- 领域流程

---

### Worker

用途：

提供受控执行能力。

示例：

- 文件处理
- 数据转换
- 外部执行

Worker 必须在 Core 权威之下执行。

---

### Connector（连接器）

用途：

提供外部系统集成。

示例：

- 服务 API
- 数据源
- 外部应用

Connector 绝不得绕过信任边界（Trust boundary）。

---

### Domain Capability（领域能力）

用途：

提供专业化工作流。

示例：

- 专业工作流
- 行业场景
- 高级自动化

---

## 5. 能力生命周期

每项能力依次经历：

定义（Define）

↓

注册（Register）

↓

验证（Validate）

↓

授权（Authorize）

↓

执行（Execute）

↓

报告（Report）

---

## 6. 能力权威

Core 负责：

- 能力注册权限
- 执行授权
- 运行时调度
- 状态转换

能力实现不拥有权威。

---

## 7. Foundation 能力

Delta Foundation 提供：

- Core 能力框架
- 基础能力
- 扩展接口

Foundation 能力必须保持通用。

---

## 8. 扩展能力

可选扩展可以提供：

- 高级能力
- 专业化工作流
- 官方维护的扩展

扩展能力必须使用 Delta SDK 与 Delta 合同（Contracts）。

---

## 9. 禁止模式

不得创建以下内容。

### Capability Core

能力绝不得演变为另一个平台。

### Capability Runtime

能力绝不得创建另一个执行引擎。

### Capability Authority

能力绝不得创建独立的权限控制。

### Capability State

能力绝不得创建独立的系统状态。
