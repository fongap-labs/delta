# ADR-0002：Foundation 与可选扩展

状态：已接受

## 背景

Delta 需要一个稳定、开放的 Foundation，同时允许可选的第一方与第三方能力各自独立演进。

## 决定

Delta Foundation 拥有完整的应用及全部产品权威。可选扩展只能通过公开 SDK、扩展 API、带版本的合同、清单以及受控的 Worker 边界进行集成。

Foundation 绝不导入扩展的实现，也绝不要求某个扩展才能启动或提供基础功能。

已安装扩展的清单隔离保存在 `<state>/extensions/<source>/` 下，使一个扩展不能覆盖另一个扩展。

## 后果

- Delta Foundation 在没有任何扩展时仍可独立使用，并且可以审计。
- 可选能力可以独立演进，而无需分叉 Core。
- 扩展不能建立平行的 Runtime 或 Trust 平面。
- 集成边界由公开的兼容合同定义，而不是由仓库目录结构定义。
