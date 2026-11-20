import { useI18n } from "@delta/i18n/I18nContext";
import type { ArtifactIntegrityStatus } from "../api";
import { summarizeBulkIntegrity, type BulkIntegrityResults, type BulkIntegrityPhase } from "../useArtifactBulkIntegrity";
import { Button } from "./ui/Button";

interface Props {
  phase: BulkIntegrityPhase;
  checked: number;
  total: number;
  results: BulkIntegrityResults;
  onStart: () => void;
  onStop: () => void;
}

export function ArtifactBulkIntegrity({ phase, checked, total, results, onStart, onStop }: Props) {
  const { t } = useI18n();
  const counts = summarizeBulkIntegrity(results);
  const isChecking = phase === "checking";
  const hasResults = checked > 0;
  return (
    <div className="mt-2 border-t border-line pt-2" data-testid="artifact-bulk-integrity">
      <div className="flex items-center justify-between gap-2">
        <div className="text-[11.5px] text-muted">{t("artifacts.bulkTitle", undefined, "Current-file integrity")}</div>
        {isChecking ? (
          <Button size="sm" variant="ghost" onClick={onStop}>
            {t("artifacts.bulkStop", undefined, "Stop")}
          </Button>
        ) : (
          <Button size="sm" variant="ghost" onClick={onStart}>
            {t("artifacts.bulkStart", undefined, "Check files")}
          </Button>
        )}
      </div>
      {phase !== "idle" && (
        <>
          <div className="mt-1 text-[11px] text-muted" data-testid="artifact-bulk-progress" aria-live="polite">
            {t("artifacts.bulkProgress", { checked, total }, checked + "/" + total + " checked")}
            {phase === "stopped" && " · " + t("artifacts.bulkStopped", undefined, "Stopped early")}
          </div>
          <progress className="w-full h-1" value={checked} max={total || 1}
            aria-label={t("artifacts.bulkProgressLabel", undefined, "File checks completed")} />
          {hasResults && (
            <div className="mt-1 flex flex-wrap gap-x-3 gap-y-1 text-[11px] text-muted" data-testid="artifact-bulk-counts">
              <span>{t("artifacts.bulkMatched", undefined, "Matched")} {counts.matched}</span>
              <span>{t("artifacts.bulkChanged", undefined, "Changed")} {counts.changed}</span>
              <span>{t("artifacts.bulkUnknown", undefined, "Not verified")} {counts.unrecorded + counts.unsupported + counts.unavailable}</span>
            </div>
          )}
          <div className="mt-1 text-[10.5px] text-muted">
            {t("artifacts.bulkDisclaimer", undefined, "Only files checked so far are counted. Hash matches do not approve delivery.")}
          </div>
        </>
      )}
    </div>
  );
}

export function ArtifactBulkMark({ status }: { status: ArtifactIntegrityStatus | "unavailable" }) {
  const { t } = useI18n();
  const labels = {
    matched: t("artifacts.bulkMatched", undefined, "Matched"),
    changed: t("artifacts.bulkChanged", undefined, "Changed"),
    unrecorded: t("artifacts.integrityUnrecorded", undefined, "No usable recorded SHA-256"),
    unsupported: t("artifacts.integrityUnsupported", undefined, "This file type cannot be compared"),
    unavailable: t("artifacts.reviewUnavailable", undefined, "Unavailable"),
  };
  return (
    <span className={"text-[10.5px] shrink-0 " + (status === "changed" ? "text-danger" : "text-muted")}
      data-testid="artifact-bulk-status" data-state={status} title={labels[status]}>
      {status === "unrecorded" || status === "unsupported" || status === "unavailable"
        ? t("artifacts.bulkUnknown", undefined, "Not verified") : labels[status]}
    </span>
  );
}
