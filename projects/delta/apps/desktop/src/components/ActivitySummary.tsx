import type { Item, SessionUsage } from "../types";
import { formatTokens, totalTokens } from "../usage";
import { formatToolDuration } from "../toolTiming";
import { summarizeActivity } from "../activityMetrics";
import { useI18n } from "@delta/i18n/I18nContext";

interface Props {
  items: readonly Item[];
  usage?: SessionUsage;
}

/** Compact, read-only facts from this session. No estimated cost or task ETA. */
export function ActivitySummary({ items, usage }: Props) {
  const { t } = useI18n();
  const stats = summarizeActivity(items);
  // An empty byModel map means the server did not provide a usage sidecar.
  const usageReported = !!usage && Object.keys(usage.byModel).length > 0;
  if (stats.total === 0 && !usageReported) return null;

  const counts = [
    [stats.completed, "artifacts.activityCompleted", "Completed"],
    [stats.failed, "artifacts.activityFailed", "Failed"],
    [stats.running, "artifacts.activityRunning", "Running"],
    [stats.interrupted, "artifacts.activityInterrupted", "Interrupted"],
    [stats.unknown, "artifacts.activityUnknown", "Without results"],
  ] as const;

  return (
    <div data-testid="rail-activity-summary" className="mb-3 border-b border-line pb-3 text-[11.5px]">
      <div className="mb-2 font-medium text-ink">{t("artifacts.activityTitle", undefined, "Session activity")}</div>
      {stats.total > 0 && (
        <>
          <div className="flex justify-between gap-2 text-muted">
            <span>{t("artifacts.activityCalls", undefined, "Recorded tool calls")}</span>
            <span data-testid="rail-activity-total" className="tabular-nums text-ink">{stats.total}</span>
          </div>
          <div className="mt-1 flex flex-wrap gap-x-3 gap-y-1 text-muted" data-testid="rail-activity-status">
            {counts.filter(([value]) => value > 0).map(([value, key, fallback]) => (
              <span key={key}>{t(key, undefined, fallback)} <span className="tabular-nums">{value}</span></span>
            ))}
          </div>
          {stats.longestMs !== null && (
            <div className="mt-2 flex justify-between gap-2 text-muted">
              <span>{t("artifacts.activityLongest", undefined, "Longest measured tool call")}</span>
              <span data-testid="rail-activity-longest" className="tabular-nums text-ink">{formatToolDuration(stats.longestMs)}</span>
            </div>
          )}
          {stats.timed < stats.total && (
            <div className="mt-1 text-faint" data-testid="rail-activity-partial">
              {t("artifacts.activityTimedCount", { n: stats.timed, total: stats.total }, stats.timed + "/" + stats.total + " calls have timing")}
            </div>
          )}
        </>
      )}
      <div className="mt-2 flex justify-between gap-2 text-muted">
        <span>{t("artifacts.activityTokens", undefined, "Reported session tokens")}</span>
        <span data-testid="rail-activity-tokens" className="tabular-nums text-ink">
          {usageReported ? formatTokens(totalTokens(usage)) : t("artifacts.activityNotReported", undefined, "Not reported")}
        </span>
      </div>
    </div>
  );
}
