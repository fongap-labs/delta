import { useCallback, useEffect, useState } from "react";
import { getApprovalRules, revokeApprovalRule, type ApprovalRule, type ApprovalRules } from "../api";
import { useI18n } from "@delta/i18n/I18nContext";

const BTN = "text-[11.5px] px-2 py-1 rounded-md border border-line bg-bg hover:border-line-strong shrink-0";

/** What the user has allowed without being asked: this session's choices and the workspace's
 *  rules. Each one can be taken back with a click, and the next matching call asks again. */
export function ApprovalRulesList({
  sessionId,
  workspace,
  refreshKey,
}: {
  sessionId: string;
  workspace: string | null;
  /** Changes when an approval was answered, which may have made a rule. */
  refreshKey: number;
}) {
  const { t } = useI18n();
  const [rules, setRules] = useState<ApprovalRules | null>(null);
  const [hasFailed, setFailed] = useState(false);

  const load = useCallback(() => {
    getApprovalRules(sessionId, workspace ?? "")
      .then((out) => {
        setRules(out);
        setFailed(false);
      })
      .catch(() => setFailed(true));
  }, [sessionId, workspace]);
  useEffect(load, [load, refreshKey]);

  const revoke = async (scope: "session" | "workspace", rule: ApprovalRule) => {
    try {
      await revokeApprovalRule(scope, sessionId, workspace ?? "", rule);
    } catch {
      setFailed(true);
      return;
    }
    load();
  };

  const rows: { scope: "session" | "workspace"; rule: ApprovalRule }[] = [
    ...(rules?.session ?? []).map((rule) => ({ scope: "session" as const, rule })),
    ...(rules?.workspace ?? []).map((rule) => ({ scope: "workspace" as const, rule })),
  ];
  if (!hasFailed && rows.length === 0) return null;

  return (
    <div className="mt-3" data-testid="approval-rules">
      <div className="text-[11px] uppercase tracking-[0.05em] text-faint font-semibold mb-1">
        {t("rules.title")}
      </div>
      {hasFailed && <div className="text-[12px] text-warn">{t("rules.error")}</div>}
      {rows.map(({ scope, rule }) => (
        <div
          key={`${scope}\0${rule.tool}\0${rule.command ?? ""}`}
          className="flex items-center gap-2 py-1"
          data-testid="approval-rule"
        >
          <span className="flex-1 min-w-0 text-[12.5px] break-all">
            {rule.tool}
            {rule.command ? <span className="text-muted"> — {rule.command}</span> : null}
          </span>
          <span className="text-[11px] text-muted shrink-0" data-testid="approval-rule-scope">
            {scope === "session" ? t("rules.scope.session") : t("rules.scope.workspace")}
          </span>
          <button
            className={BTN}
            data-testid="approval-rule-revoke"
            title={t("rules.revokeHelp")}
            onClick={() => void revoke(scope, rule)}
          >
            {t("rules.revoke")}
          </button>
        </div>
      ))}
      {rules?.workspace_error && <div className="text-[12px] text-warn">{t("rules.error")}</div>}
    </div>
  );
}
