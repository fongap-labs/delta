# Delta 品牌资产清单

`resources/brand/` 是 Delta 正式 Logo 母版的唯一来源。历史版本资源保留在 Git 历史分支中，不在当前产品品牌目录下区分 Legacy。

## 母版

- `delta-logo-512x512.png`：512px Delta 官方 Logo。
- `delta-logo-256x256.png`：256px Delta 官方 Logo。

## 当前生成或引用的产物

- `apps/desktop/resources/icon.png`、`icon-windows.png`、`icon-macos.png`。
- `apps/desktop/renderer/assets/delta-logo-512x512.png`。
- `packages/client/ui-brand-official/src/client/assets/delta-logo-256x256.png`（只为 Delta 侧边栏插件保留，必须与 256px 母版一致）。
- `packaging/portable/launcher/icon.ico`（独立图标格式，须通过实际便携包验证）。

上游鲸鱼帧图和首页 Canvas 彩蛋已移除，Delta 首页渲染不依赖它们。修改母版后必须同步更新引用产物，并通过 `scripts/delta-verify.mjs` 中的品牌一致性断言。不得因目录迁移自行删除真实桌面安装包所需的图标。
