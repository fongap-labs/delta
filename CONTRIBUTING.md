# 参与开发

## 1. 项目边界

不得建立第二套运行时权威。

### ❌ 错误做法 / 反面教材

让扩展直接写核心状态。

```text
插件 → 绕过审批 → 修改核心数据库
```

### ✅ 正确做法 / 规范范例

扩展通过既有能力与公共合同。

```text
界面 → 能力接口 → Rust 运行时 → 权限检查与持久化
```

界面使用 TypeScript，运行与审批由 Rust 负责；受控脚本不能替代权威。优先复用能力和技能，必要时新增执行器。

## 2. 提交规范

提交标题只允许三种类型。

### ❌ 错误做法 / 反面教材

继续使用旧类别。

```text
docs: update README
refactor: improve runtime
```

### ✅ 正确做法 / 规范范例

英文标题、受限类别、签署提交。

```bash
git switch -c chore/governance-docs-align
git commit -s -m "chore(governance): align shared rules"
```

仅使用 `feat`、`fix`、`chore`；破坏性变更按中央规范标记。贡献必须通过合并请求及中央门禁。

## 3. 中文变更记录

新增和修复必须写入统一日志。

### ❌ 错误做法 / 反面教材

继续提交片段。

```text
changelog.d/runtime-update.md
```

### ✅ 正确做法 / 规范范例

直接更新未发布章节。

```markdown
## [Unreleased]

- 修复: 修复会话压缩后上下文缺失的问题。
```

类别仅用新增、修复、维护；摘要不超过八十字。维护变更可不记录。权威为 [中央变更规范](https://github.com/fongap-labs/action-worker/blob/main/docs/04_CHANGELOG_CONVENTIONS.md)。

## 4. 验证与交接

涉及审批和恢复的改动必须回归验证。

### ❌ 错误做法 / 反面教材

跳过失败测试或扩大权限绕过。

```text
运行失败 → 删除测试 → 合并
```

### ✅ 正确做法 / 规范范例

先确认当前合同再执行相关测试。

```text
读取公共合同 → 修改实现 → 验证失败路径
             → 运行中央测试 → 记录证据 → 合并
```

公共合同见 [运行时契约](docs/architecture/runtime-public-contract.md)，架构决策见 [决策索引](docs/architecture/adr/README.md)，项目治理见 [治理索引](docs/governance/README.md)。不得把历史待办当作当前规范。
