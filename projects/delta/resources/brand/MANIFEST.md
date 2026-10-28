# Delta 品牌清单

本文说明 `resources/brand/` 中的品牌源文件与由其生成的产物之间的对应关系。

`resources/brand/` 是 Delta 品牌的**唯一事实来源（single source of truth）**。

## 规范来源

- `delta-logo-512x512.png`：母版 Logo 位图。所有生成的 Logo 衍生文件均由此文件追溯。
- `delta-logo-256x256.png`：母版 Logo 的 256px 便捷导出版本。

品牌资产必须在**此处**编辑（或在上游编辑后同步到此处），不得手工修改生成的图标。生成的图标资产请使用 `scripts/check_brand_icons.py` 检查。

## 生成产物位置（可追溯至源文件）

| 位置 | 用途 |
| --- | --- |
| `apps/desktop/src-tauri/icons/` | 桌面应用图标：`icon.png`、`icon.ico`、`icon.icns`、Tauri 方形与商店 Logo（`32x32.png` … `Square310x310Logo.png`、`StoreLogo.png`）、`tray.png` / `tray.rgba`（全彩，适用于 Windows 与 Linux），以及 `tray-template.png` / `tray-template.rgba`（macOS 菜单栏单色模板，由 `python scripts/brand_tray_template.py` 生成） |
| `packaging/portable/launcher/icon.ico` | 便携版启动器的窗口图标 |

使用 `python scripts/check_brand_icons.py --verify` 确认每个生成产物均已存在，且比品牌源文件新。
