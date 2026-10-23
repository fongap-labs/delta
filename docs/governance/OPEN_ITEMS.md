# 未决事项（Open Items）

本文记录需要所有者决定、法务审查或另行处理、且本仓库本次没有结论的事项。每一项写明现状与需要的输入，不作替代判断。

## 1. 审计文档缺失（已移除引用）

- 原 `scripts/check_provenance_policy.py` 的 `LEGAL_AND_HISTORY` 列表引用两份不存在的文件：`docs/audits/phase-1-provenance-license-audit.md` 与 `docs/audits/phase-2.5-foundation-cleanup-audit.md`。这两份文件从未进入本仓库的任何提交（已取完整历史，并检查了 origin 上的全部分支），本机也未找到副本。
- 所有者决定：移除脚本中对这两份文件的引用。脚本不再声称存在这两份审计，也不再因此通过。
- 后续：需要时，基于仓库当前状态另行撰写带日期与审查范围的审计记录，并让脚本指向新文件。不得根据不存在的旧审计补写结论。
- 另一个会话的 scratchpad 中存在的跨仓库 `audit-report.md` 未被导入，其中关于 delta 的条目未经验证。

## 2. 法务审查（不作结论）

以下事项包含法律或合规判断，需要法务人员审查。本仓库不对其作出法律结论。

- `docs/governance/baseline/SOURCE_LICENSING_GOVERNANCE.md`：强 Copyleft 或源码可见许可的依赖，若对 Foundation 施加不兼容的分发义务，须经明确的法务审查。
- `docs/governance/baseline/SECURITY_GOVERNANCE.md`：关于"不对安全研究者提起法律诉讼"的承诺，需确认措辞、适用范围与对应条件。
- `docs/governance/baseline/TELEMETRY_PRIVACY_GOVERNANCE.md`：GDPR 法律依据（标准遥测的合法利益、增强与内容遥测的同意）、数据保护官任命与第 28 条处理者要求。
- `docs/governance/baseline/COMPLIANCE_EXPORT_GOVERNANCE.md`：出口管制、制裁筛查与再出口义务。
- `docs/governance/baseline/COMMUNITY_MAINTAINER_GOVERNANCE.md`：商标侵权时由项目负责人授权提起法律行动的流程。

## 3. 占位联系方式

- 已将治理文档中的占位邮箱（`security@`、`conduct@`、`community@`、`extensions@` 的 `delta.example.com` 地址）替换为 `info@fongap.com`。
- 未替换的占位 URL：`docs/governance/baseline/TELEMETRY_PRIVACY_GOVERNANCE.md` 中的透明度报告地址 `https://delta.example.com/transparency`。它是 URL，不是联系邮箱，真实地址需所有者提供。
- 未替换的示例地址：`packages/i18n/*.ts` 中的输入框示例 `name@example.com`，以及测试与样例数据（如 `ada@example.com`、`pass@example.com`、`127.0.0.1@example.com`）。它们不是联系地址。

## 4. 扩展加载

见 [docs/architecture/extension-loading.md](../architecture/extension-loading.md) 第 5 节：清单 schema 缺失、两种 worker manifest 形状并存、`delta_extension_api` 仍依赖 Foundation 内部、清单未声明目标 ABI。

## 5. ~~既存失败的检查~~（已解决）

以下检查曾在本仓库 `main` 上失败，现已修复：

- `scripts/check_repository_boundary.py`：已通过（`Delta Foundation repository boundary: OK`）。
- `scripts/check_naming.py`：已通过（`naming conventions passed`），PR #200 修正了 Rust 测试模块的排除规则。

## 6. Rust 依赖公告例外

RustSec 于 2026-10-17 与 2026-10-19 两批公告使 `cargo deny check advisories` 在四个工作区失败。`deny.toml` 现接受以下例外，每条都写明影响、可利用性、缓解措施与后续计划；复审日期 **2027-01-31**。

### 6.1 zip 提取类漏洞（RUSTSEC-2026-0367 / 0368 / 0369）

- 现状：受影响的版本是全部 `zip < 9.0.0`。Delta 锁定两个版本：`sherpa-onnx-sys` 引入的 `zip 2.4.2`，以及 `tauri-plugin-updater` 引入的 `zip 4.6.1`。两个上游 crate 当前都没有把依赖抬到 `zip 9` 的发布版本，因此单靠 `cargo update` 无法消除。
- 可利用性：三条公告都只影响 `ZipArchive::extract()` / `ZipStreamReader::extract()` 这条落盘路径。Delta 自身代码不调用 `extract()`：Office 文档读取只在内存中按名字取条目（`crates/delta-core/src/document/ooxml.rs`），Skill 归档展开使用 `enclosed_name()` 并自行写文件（`crates/delta-core/src/skills.rs`），已阻断路径穿越。仓库内唯一的 zip 落盘提取来自自动更新插件，而它只安装经 minisign 公钥校验过的发布产物（`apps/desktop/src-tauri/tauri.conf.json` 的 `pubkey`），攻击者需要签名私钥才能构造被安装的包。
- 缓解措施：更新链路的签名校验不得放宽；用户须显式确认安装更新（非静默后台安装）；更新链路的签名校验不得放宽。
- 后续：① `sherpa-onnx-sys` 发布依赖 `zip >= 9` 的版本后，删除 `crates/delta-stt` 的 `zip 2.x`；② `tauri-plugin-updater`（或 Tauri 3 系列）改用 `zip >= 9` 后，删除 `apps/desktop/src-tauri` 的 `zip 4.x`；③ 把 `crates/delta-core` 自身声明的 `zip = "2"` 升到 `9`，使 `delta-core` 与 `delta-computer` 工作区不再出现受影响的版本；④ 三条公告的例外在对应版本消失后一并删除。

### 6.2 zvariant 系列停止维护（RUSTSEC-2026-0370 / 0371 / 0372）

- 现状：`zvariant`、`zbus_names`、`zvariant_derive` 被标记为停止维护，全部版本都受影响。它们经 `zbus 5.19.0` 引入，而 `zbus` 由 `tauri-plugin-single-instance` 与 `tauri-plugin-notification` 依赖，当前插件版本仍要求 `zbus ^5`。
- 可利用性：停止维护不等于漏洞，这三条公告没有可利用的缺陷，只表示上游不再修复。
- 缓解措施：不降低 `unmaintained = "all"` 的整体口径，例外只针对这三个 crate。
- 后续：等 Tauri 插件支持 `zbus 6.0.0`（`zbus 6` 已移除 `zvariant` / `zbus_names`）后升级插件，并删除这三条例外；在此之前保持当前 Tauri 版本，不为消除告警而引入 alpha 插件。

### 6.3 根证书信任链类漏洞（RUSTSEC-2026-0381 / 0382 / 0383）

- 现状：三条公告都指向 TLS 根证书与校验链。`RUSTSEC-2026-0382`（`rustls-webpki` 可能接受不受信的中间证书）与 `RUSTSEC-2026-0383`（`webpki-root-certs` 打包了已被 distrusted 的 emSign 根）已随锁文件把 `rustls-webpki` 升到 0.103.16、`webpki-root-certs` 升到 1.0.10、`webpki-roots` 升到 1.0.10 消除。`RUSTSEC-2026-0381`（`webpki-roots` 打包了已被 distrusted 的 emSign 根）仍剩一处：Delta 直接依赖 `ureq 2`，它引入 `webpki-roots 0.26.11`；该 wrapper 本身依赖 `webpki-roots ^1.0`，锁文件现已解析到修复版 1.0.10，但 `ureq 2.12.1` 是 2.x 的最后一个版本，钉死 `webpki-roots ^0.26`，而 0.26.x 没有修复版（修复从 1.0.10 起）。
- 可利用性：需要 emSign 中间证书的私钥（已公开）才能构造被误信的证书链，属于中间人攻击，条件苛刻但不为零。Delta 桌面端的网络访问经 Tauri 插件，模型与浏览器请求走各自的客户端；受影响的 `ureq` 只用于 MCP 运行时与模型权威的出站请求。
- 缓解措施：锁文件把 `webpki-roots` 固定在 1.0.10，实际使用的根证书集已不含该根；`ureq` 的出站请求已关闭重定向并对解析出的地址做固定。
- 后续：① 把 `crates/delta-core` 的 `ureq = "2"` 迁到 `ureq 3`（`ureq 3` 依赖 `webpki-roots 1.x`），随后删除本例外；② 迁移要改写 `crates/delta-core/src/mcp_runtime.rs` 的 TLS 连接器实现（`ureq::TlsConnector` 与 `ReadWrite` 在 3.x 改了 trait 形状），且必须保留现有的地址固定与重定向关闭语义；③ 迁移完成后本节与 `deny.toml` 中的 0381 例外一并删除。
