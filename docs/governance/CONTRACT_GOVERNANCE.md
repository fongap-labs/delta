# Delta 合同治理（Contract Governance）

版本：1.0

状态：架构基线（Architecture Baseline）

本文规定公开合同的范围、兼容性规则、SDK 边界，以及修改合同前的 AI 代理规则。

## 1. 目的

合同定义以下三者之间的稳定边界：

- Delta Foundation
- Delta Suite
- 外部扩展（External Extensions）

合同比实现更稳定。

---

## 2. 合同原则

Delta 遵循：

**Contract over Implementation（合同优先于实现）**

消费方依赖：

- 接口（Interfaces）
- Schema
- 合同

消费方绝不得依赖：

- 内部模块
- 私有实现细节

---

## 3. 合同范围

合同定义以下内容。

### Capability 合同

定义：

- Capability 身份
- 输入
- 输出
- 权限要求

### Worker 合同

定义：

- 执行接口
- 状态报告
- Artifact 输出

### Connector 合同

定义：

- 外部集成边界

### Artifact 合同

定义：

- 数据身份
- 引用
- 来源证明（provenance）

### Event 合同

定义：

- 系统事件
- 状态变更

---

## 4. 兼容性规则

合同变更需要评审。

允许：

- 新增可选字段
- 扩展 Capability 元数据
- 新增接口

需要评审：

- 删除字段
- 改变行为
- 破坏兼容性

---

## 5. SDK 边界

SDK 提供：

- 扩展接口
- 合同访问
- 开发工具

SDK 不暴露：

- Core 内部实现
- 权威内部机制
- 私有状态管理

---

## 6. AI 代理规则

修改合同之前：

1. 检查现有使用方。
2. 考虑兼容性。
3. 优先扩展。
4. 避免破坏消费方。

合同是架构资产，不得随意修改。

---

## 7. 语言例外

Action Worker 共用治理（`docs/SHARED_GOVERNANCE.md` 第 8.1 节）把 `projects/delta/contracts/` 列为机器读取文档，要求英文。Delta 对此设立一项由所有者决定的例外：

1. `projects/delta/contracts/README.md` 是面向人的文档，使用中文撰写，且只保留这一份。
2. `projects/delta/contracts/` 中的机器读取合同规则与数据形状，全部保留在英文文件 `projects/delta/contracts/CONTRACT_RULES.md` 中。该文件是唯一权威来源；`README.md` 不得重复其中的规则。
3. `projects/delta/contracts/` 中新增的其他文件，仍按第 8.1 节使用英文，除非所有者另行明确列为例外。

该例外与共用治理的字面要求不一致。在共用治理修订之前，它只适用于上述两个文件，不得据此放宽其他机器读取文档的语言要求。
