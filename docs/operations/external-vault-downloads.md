# 依赖包下载（external-vault）

本文说明如何手动下载 Delta 统计功能依赖的 R 和 R 包。这些文件发布在 [fongap-labs/external-vault](https://github.com/fongap-labs/external-vault/releases) 的 GitHub Releases 中（Release 是 GitHub 上带版本号的文件下载页）。

## 快速使用

在仓库根目录运行：

```text
python3 scripts/fetch_external_vault.py --list              # 列出固定的工具和下载地址
python3 scripts/fetch_external_vault.py r-base              # 只下载 R 4.6.0 源码包
python3 scripts/fetch_external_vault.py                     # 下载全部
python3 scripts/fetch_external_vault.py --dest ./vault qcc  # 指定保存目录
```

脚本会：

1. 按 `<工具名>-v<版本>` 找到 Release，下载对应文件；
2. 用 `.github/external-vault-assets.json` 中固定的 SHA-256 校验（SHA-256 是文件内容的指纹，文件被改动哪怕一个字节都会不同）；
3. 校验通过后，把保存目录打印到标准输出。

校验不通过的文件会被删除，脚本以失败退出，不会继续使用来路不明的文件。

## 已固定的文件

| 工具名 | 版本 | 文件 | 用途 |
|--------|------|------|------|
| `r-base` | 4.6.0 | `R-4.6.0.tar.xz` | R 源码；统计功能只接受 R 4.6.0（ADR-0054） |
| `doe-base` | 1.2.5 | `DoE.base_1.2-5.tar.gz` | R 包，试验设计 |
| `frf2` | 2.3.5 | `FrF2_2.3-5.tar.gz` | R 包，二水平部分因子设计 |
| `rsm` | 2.10.6 | `rsm_2.10.6.tar.gz` | R 包，响应曲面 |
| `algdesign` | 1.2.1.2 | `AlgDesign_1.2.1.2.tar.gz` | R 包，最优设计 |
| `qcc` | 2.7.0 | `qcc_2.7.tar.gz` | R 包，质量控制图 |
| `blandr` | 0.6.0 | `blandr_0.6.0.tar.gz` | R 包，Bland-Altman 分析 |
| `bland-altman-leh` | 0.3.1 | `BlandAltmanLeh_0.3.1.tar.gz` | R 包，Bland-Altman 分析 |

目前 Delta 本体只直接要求 R；R 包随统计能力的拆分（ADR-0071）一并固定，便于离线准备。

## 没有网络时（手动下载）

如果运行脚本的机器不能访问 GitHub：

1. 在能访问的机器上打开 `--list` 打印的地址，把文件保存下来；
2. 把文件复制到目标机器的保存目录（默认 `<系统临时目录>/delta-external-vault`，或 `--dest` 指定的目录）；
3. 运行同样的命令。脚本发现目录里已有哈希匹配的文件，就不会再下载，只做校验。

## 维护者：升级版本

1. 在 external-vault 的 `tools/<工具名>/tool.json` 确认新的版本、文件名和 `upstream_sha256`，并确认对应 Release 已发布；
2. 修改 `.github/external-vault-assets.json` 中该工具的 `version`、`asset`、`sha256`；
3. 运行 `python3 scripts/fetch_external_vault.py <工具名>` 确认下载与校验通过。

没有固定 SHA-256 的文件会被拒绝，所以升级必须同时更新校验值。

## 与 CI 的关系

当前 `.github/scripts/central-ci.sh` 仍直接从 CRAN 下载 R 源码并校验同一个 SHA-256，本次没有改动。两处的校验值必须保持一致；改为从 external-vault 下载属于后续变更。
