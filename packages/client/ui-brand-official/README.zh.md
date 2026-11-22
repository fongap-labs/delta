---
description: "Delta 侧边栏品牌渲染组件。"
kind: "package-reference"
---

# @deepseek-ai/dsh-client-ui-brand-official

[English](README.md) | 中文

## Summary

本包通过 `sidebar.brand.mark` 与 `sidebar.brand.name` 呈现 Delta 标志和名称，不包含上游鲸鱼动画、远程反馈或遥测。

## Assets

Delta Logo 母版统一存放在 `resources/brand/`。`src/client/assets/` 中的 256px 副本仅用于浏览器打包，必须与母版保持一致。

## Runtime

Host 部分不执行操作；Client 部分只填充两个侧边栏插槽，不修改会话首屏或首页回退渲染。默认 Web 装配不需要激活此包。

## Model Experience

无。此组件仅负责 UI 渲染，不发起模型或网络请求。

## Known Limitations and Deferred Work

继续使用现有的两个插槽约定及 Delta Logo 打包副本。后续品牌迁移必须保留会话首屏的默认渲染。
