# Delta 品牌与资产规范

## 既有资产复用

Delta 继续沿用 `delta-legacy` 已批准的 Logo，不重新设计或生成另一个视觉标识。源文件以 [旧品牌清单](https://github.com/fongap-labs/delta-legacy/blob/main/projects/delta/resources/brand/MANIFEST.md) 为迁移依据：

- [`delta-logo-512x512.png`](https://github.com/fongap-labs/delta-legacy/blob/main/projects/delta/resources/brand/delta-logo-512x512.png)：母版位图。
- [`delta-logo-256x256.png`](https://github.com/fongap-labs/delta-legacy/blob/main/projects/delta/resources/brand/delta-logo-256x256.png)：便捷导出版本。

迁移到 Delta 后，在本仓库为母版设立唯一事实来源，所有 Tauri/Electron、系统托盘和安装包图标由母版派生并通过自动校验；禁止逐个平台手工改 Logo。

## 品牌色

| 场景 | 主色 | 来源 |
| --- | --- | --- |
| 浅色模式 | `#286F78` | `delta-legacy/projects/delta/apps/desktop/src/styles.css` 的 `--primary` |
| 深色模式 | `#60B4BF` | 同文件 `html[data-theme="dark"]` 的 `--primary` |

中性色、悬停、选中与焦点状态继续从旧 Delta 的语义设计 token 迁移；避免在组件中散落硬编码颜色。Logo 既有视觉比例与透明度保持不变。

本文件先确定来源和迁移规则；当前 PR 仅建立文档基线，二进制图片及生成图标将在 Desktop 迁移时原样复制并校验，不声称已落入本仓库。

