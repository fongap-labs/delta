# 项目治理索引

## 1. 共用权威

组织规则不得在项目内重复定义。

### ❌ 错误做法 / 反面教材

Delta 单独规定提交类型与通用合并门禁。

```text
Delta 自定义提交类别 → 覆盖 Action Worker
```

### ✅ 正确做法 / 规范范例

所有项目统一引用中央规范。

```text
fongap-labs/action-worker/docs/01_SHARED_GOVERNANCE.md
fongap-labs/action-worker/docs/04_CHANGELOG_CONVENTIONS.md
```

## 2. 项目权威

Delta 只维护自身产品与运行边界。

### ❌ 错误做法 / 反面教材

让实施说明代替公共运行时合同。

```text
临时实施计划 → 覆盖运行时合同
```

### ✅ 正确做法 / 规范范例

按领域阅读唯一权威。

```text
产品边界 → ../DELTA_BLUEPRINT.md
目标架构 → ../architecture/target-architecture.md
公共运行时 → ../architecture/runtime-public-contract.md
能力接口 → ../architecture/capability-abi.md
仓库边界 → REPOSITORY_BOUNDARY.md
合同规则 → CONTRACT_GOVERNANCE.md
质量规则 → QUALITY_POLICY.md
依赖规则 → DEPENDENCY_POLICY.md
发布边界 → RELEASE_POLICY.md
```

详细决策以 `../architecture/adr/README.md` 为索引。历史方案与专项基线不得成为第二套执行权威。

## 3. 变更验证

合同变更必须先验证消费者和门禁。

### ❌ 错误做法 / 反面教材

只改文档而跳过受影响测试。

```text
更新接口描述 → 直接合并
```

### ✅ 正确做法 / 规范范例

代码、测试与说明同步。

```text
公共合同 → 消费方检查 → 回归测试 → 中央合并门禁
```

Delta 的 Rust 运行时保留唯一执行与审批权威；扩展不得建立第二套状态。
