# ADR-0083：Delta / Delta Pro 只由 About 的 edition 标记区分

状态：**已接受**（所有者，2026-10-27，要求不作为单独安装包，只在关于界面显示）。

## 背景

- 产品上只有一个 Delta 应用：不安装 Delta Suite 时是 Delta；安装并生效后是 Delta Pro。
- 不需要两个安装包、两套更新通道或两套 UI；差异只出现在“关于”界面。
- Foundation 不能写死 `delta-suite` 或“如果装了 Suite”的分支，否则会破坏 ADR-0082 的干净切割。

## 决定

### 1. 标记文件

可选扩展可以在自己的已安装目录下写一个 `product-edition.json`：

```json
{
  "format": "delta.product-edition",
  "version": 1,
  "edition": "pro",
  "label": "Delta Pro",
  "source": "delta-suite",
  "rank": 100
}
```

- `source` 必须等于所在扩展目录名。
- 同目录必须有 `capability-workers.json`，否则标记被忽略。
- Foundation 只读取 `format`、`version`、`edition`、`label`、`source` 与可选 `rank`；文件无效时按 Delta 处理。
- 多个标记时取 `rank` 最高者；同 rank 取目录名排序靠前者。

### 2. 显示规则

- 没有有效标记：关于界面显示 **Delta**。
- 有有效标记：显示标记中的 `label`（当前为 **Delta Pro**）。
- 版本号来自应用自身的 `package_info`，不来自标记文件。
- 标记只影响关于界面，不改变功能、权限、审批、能力注册或报告门禁。

### 3. 安装与移除

- Suite 安装器写入该文件；删除 Suite 扩展目录即移除标记。
- 不需要单独的卸载器；不写 Foundation 状态库。
- 标记不是权益凭证。若未来需要付费权益判断，必须另行设计签名或授权校验，不能依赖这个文件。

## 后果

- Foundation 用通用目录扫描读取标记，源码不出现 Suite 实现标识。
- 同一个应用、同一个安装包可以显示 Delta 或 Delta Pro。
- 新增 `schemas/product-edition.schema.json` 与关于界面的产品信息读取命令。
- 伪造成本很低，但只影响显示；不作为安全或商业权益边界。

## 未验证事项

- 关于界面的实际布局与间距需要桌面端人工检查。
- 标记文件在 Windows 与 Linux 上的路径大小写行为需要一致处理（当前按目录名精确比较）。
