# 未决事项（Open Items）

本文记录需要所有者决定、法务审查或另行处理、且本仓库本次没有结论的事项。每一项写明现状与需要的输入，不作替代判断。

## 1. 审计文档缺失

- `scripts/check_provenance_policy.py` 的 `LEGAL_AND_HISTORY` 列表引用两份文件：`docs/audits/phase-1-provenance-license-audit.md` 与 `docs/audits/phase-2.5-foundation-cleanup-audit.md`。
- 这两份文件从未进入本仓库的任何提交。已取完整历史（143 个提交），并检查了 origin 上的 `main`、`claude/compassionate-dirac-29hfcp` 与 `claude/sharp-goodall-0cyamn` 分支，均不存在。本机也未找到副本。
- 本仓库中唯一被删除的治理文档（`DELTA_GOVERNANCE.md`、`AI_AGENT_INSTRUCTIONS.md`、`AI_AGENT_RULES.md`、`DEVELOPMENT_POLICY.md`、`baseline/ARCHITECTURE_GOVERNANCE.md`）是为消除与共用治理的重复而删除的，不是审计文档，也不应恢复。
- 另一个会话的 scratchpad 中存在名为 `audit-report.md` 的跨仓库文档审计（英文，2026-10-13）。它不在任务指定的路径，覆盖范围是部分阅读，其中关于 delta 的条目未经验证。它未被导入。
- 需要所有者提供：原始的两份审计文档，或决定是否导入上述跨仓库审计（以及导入后的文件名与中文化范围）。在此之前，`check_provenance_policy.py` 中的引用指向不存在的文件。

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

## 5. 既存失败的检查

以下检查在本次修改之前已经失败，本次未修改其对应代码：

- `scripts/check_repository_boundary.py`：报告 `integrations/connectors/gcal_accounts.py`、`gmail_accounts.py`、`hubspot_portals.py` 缺少迁移后的公开合同（退出码 1）。
- `scripts/check_naming.py`：报告若干 Rust 与 Python 函数名超过三段（退出码 1）。
