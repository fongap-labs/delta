import { useEffect, useState } from "react";
import { getAudit, type AuditEvent } from "../api";
import { auditArgs, auditLine, auditState, auditWhen, type AuditState } from "../auditPlain";
import { PanelHead } from "./IntegrationsView";
import { LoadError, LoadingRows } from "./ViewStates";
import { useI18n } from "@delta/i18n/I18nContext";

// Activity — what Delta has done for the user, in plain words: one sentence and one status per
// action. The technical fields (session, connector, raw status, sanitized arguments) sit behind a
// "Technical details" disclosure. Read-only and filterable.
const CARD = "rounded-xl2 border border-line bg-panel";
const INPUT = "px-3 py-1.5 rounded-lg border border-line bg-bg text-[13px] text-ink outline-none focus:border-primary";
const BTN_ACCENT = "text-[12.5px] px-3 py-1.5 rounded-lg bg-primary text-on-primary shrink-0";

export function AuditView() {
  const { t } = useI18n();
  const [events, setEvents] = useState<AuditEvent[]>([]);
  const [sessionFilter, setSessionFilter] = useState("");
  const [connectorFilter, setConnectorFilter] = useState("");
  const [toolFilter, setToolFilter] = useState("");
  // Loading, failed and empty are three different things: a failed request must not read as
  // "no records".
  const [isLoading, setLoading] = useState(true);
  const [hasFailed, setFailed] = useState(false);

  const refresh = () => {
    setFailed(false);
    setLoading(true);
    return getAudit({
      limit: 150,
      session_id: sessionFilter.trim() || undefined,
      connector: connectorFilter.trim() || undefined,
      tool: toolFilter.trim() || undefined,
    })
      .then(setEvents)
      .catch(() => setFailed(true))
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    refresh();
  }, []);

  return (
    <main className="flex-1 min-w-0 flex bg-bg">
      <div className="flex-1 min-w-0 overflow-y-auto hairline-scroll">
        <div className="max-w-4xl mx-auto px-7 py-6">
          <PanelHead
            title={t("nav.activity", undefined, "Activity")}
            sub={t("activity.sub", undefined, "Recent connector and browser tool activity. Arguments are sanitized before storage.")}
          />

          <div className="flex items-center gap-2 flex-wrap mb-4">
            <input className={INPUT} placeholder={t("activity.filter.sessionId", undefined, "session id")} value={sessionFilter} onChange={(e) => setSessionFilter(e.target.value)} />
            <input className={INPUT} placeholder={t("activity.filter.connector", undefined, "connector")} value={connectorFilter} onChange={(e) => setConnectorFilter(e.target.value)} />
            <input className={INPUT} placeholder={t("activity.filter.tool", undefined, "tool")} value={toolFilter} onChange={(e) => setToolFilter(e.target.value)} />
            <button className={BTN_ACCENT} onClick={refresh}>
              {t("activity.filter.submit", undefined, "Filter")}
            </button>
          </div>

          {hasFailed ? (
            <LoadError what={t("state.what.activity", undefined, "activity")} onRetry={refresh} />
          ) : isLoading && events.length === 0 ? (
            <LoadingRows />
          ) : events.length === 0 ? (
            <div className={CARD + " p-4 text-[13px] text-muted"}>{t("activity.empty", undefined, "No audit events yet.")}</div>
          ) : (
            <div className="space-y-2">
              {events.map((ev) => (
                <AuditRow ev={ev} key={ev.id} />
              ))}
            </div>
          )}
        </div>
      </div>
    </main>
  );
}

function AuditRow({ ev }: { ev: AuditEvent }) {
  const { t } = useI18n();
  const line = auditLine(ev);
  const state = auditState(ev);
  const args = auditArgs(ev.args);
  const reason = ev.reason || ev.result_preview;
  return (
    <div className={CARD + " p-3.5"}>
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <div className="text-[14px] font-medium text-ink break-words">{t(line.key, line.params)}</div>
          <div className={"text-[12px] mt-0.5 " + STATE_TONE[state]}>{stateText(state, ev, t)}</div>
          {state === "waiting" && (
            <div className="text-[12px] text-muted mt-0.5">{t("activity.reason.approval", undefined, "This runs only after you approve it.")}</div>
          )}
        </div>
        <span className="text-[12px] text-faint shrink-0 whitespace-nowrap">{auditWhen(ev.timestamp)}</span>
      </div>
      <details className="mt-2">
        <summary className="text-[12px] text-faint cursor-pointer select-none">
          {t("activity.details", undefined, "Technical details")}
        </summary>
        <dl className="mt-2 grid grid-cols-[max-content_1fr] gap-x-3 gap-y-1 text-[11.5px] text-muted">
          <dt>{t("activity.detail.session", undefined, "Conversation")}</dt>
          <dd className="font-mono break-all">{ev.session_id || "-"}</dd>
          <dt>{t("activity.detail.tool", undefined, "Tool")}</dt>
          <dd className="font-mono break-all">{ev.tool || "-"}</dd>
          <dt>{t("activity.detail.connector", undefined, "App connector")}</dt>
          <dd className="break-all">{ev.connector || "-"}</dd>
          <dt>{t("activity.detail.status", undefined, "Recorded status")}</dt>
          <dd className="break-all">{[ev.stage, ev.status, ev.approval].filter(Boolean).join(" · ") || "-"}</dd>
          {ev.resource && (
            <>
              <dt>{t("activity.row.resourcePrefix", undefined, "resource")}</dt>
              <dd className="break-all">{ev.resource}</dd>
            </>
          )}
          {Object.keys(args).length > 0 && (
            <>
              <dt>{t("activity.detail.arguments", undefined, "Sent")}</dt>
              <dd className="font-mono break-all">{formatAuditArgs(args)}</dd>
            </>
          )}
          {reason && (
            <>
              <dt>{t("activity.detail.note", undefined, "Note")}</dt>
              <dd className="break-all">{reason}</dd>
            </>
          )}
        </dl>
      </details>
    </div>
  );
}

const STATE_TONE: Record<AuditState, string> = {
  waiting: "text-warn",
  approved: "text-ok",
  denied: "text-muted",
  done: "text-ok",
  failed: "text-danger",
  other: "text-muted",
};

function stateText(state: AuditState, ev: AuditEvent, t: (key: string, params?: Record<string, string>, fallback?: string) => string) {
  switch (state) {
    case "waiting":
      return t("activity.status.waiting", undefined, "Waiting for your approval");
    case "approved":
      return t("activity.status.approved", undefined, "You approved");
    case "denied":
      return t("activity.status.denied", undefined, "You declined");
    case "done":
      return t("activity.status.done", undefined, "Done");
    case "failed":
      return t("activity.status.failed", undefined, "Failed");
    default:
      return t("activity.status.other", { status: ev.status || ev.stage || "-" }, `Status: ${ev.status || ev.stage || "-"}`);
  }
}

function formatAuditArgs(args: Record<string, unknown>) {
  return Object.entries(args)
    .map(([k, v]) => `${k}=${typeof v === "string" ? v : JSON.stringify(v)}`)
    .join("  ");
}
