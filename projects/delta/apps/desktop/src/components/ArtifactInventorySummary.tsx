import { useI18n } from "@delta/i18n/I18nContext";
import type { ArtifactInventoryReview, ArtifactReviewStatus } from "../artifactInventory";
import { SegmentedControl } from "./ui/SegmentedControl";

interface Props {
  phase: "idle" | "loading" | "loaded" | "unavailable";
  review: ArtifactInventoryReview | null;
  isAttentionOnly: boolean;
  onAttentionChange: (isNextAttentionOnly: boolean) => void;
}

/** Read-only producing-run evidence. Never a live validation of workspace bytes. */
export function ArtifactInventorySummary({ phase, review, isAttentionOnly, onAttentionChange }: Props) {
  const { t } = useI18n();
  if (phase === "idle") return null;
  return (
    <div data-testid="artifact-inventory-summary" className="mb-2 border-b border-line pb-2">
      <div className="mb-1 text-[11.5px] text-muted">
        {t("artifacts.inventoryTitle", undefined, "Recorded artifact checks")}
      </div>
      {phase === "loading" ? (
        <div className="text-[11px] text-muted">{t("artifacts.reviewWaiting", undefined, "Checking…")}</div>
      ) : phase === "unavailable" ? (
        <div data-testid="artifact-inventory-unavailable" className="text-[11px] text-muted">
          {t("artifacts.inventoryUnavailable", undefined, "Recorded checks could not be loaded")}
        </div>
      ) : review && (
        <>
          <div className="flex flex-wrap gap-x-3 gap-y-1 text-[11px] text-muted" data-testid="artifact-inventory-counts">
            <span>{t("artifacts.inventoryPassed", undefined, "Recorded pass")} <span className="tabular-nums">{review.passed}</span></span>
            <span>{t("artifacts.inventoryFailed", undefined, "Recorded failure")} <span className="tabular-nums">{review.failed}</span></span>
            <span>{t("artifacts.inventoryMissing", undefined, "No record")} <span className="tabular-nums">{review.unrecorded}</span></span>
            {review.priorVersions > 0 && (
              <span>{t("artifacts.inventoryPrior", undefined, "Earlier versions")} <span className="tabular-nums">{review.priorVersions}</span></span>
            )}
          </div>
          <SegmentedControl
            className="mt-2"
            label={t("artifacts.inventoryFilter", undefined, "Filter artifact checks")}
            value={isAttentionOnly ? "attention" : "all"}
            options={[
              { value: "all", label: t("artifacts.inventoryAll", undefined, "All") },
              { value: "attention", label: t("artifacts.inventoryAttention", undefined, "Needs review") },
            ]}
            onChange={(value) => onAttentionChange(value === "attention")}
          />
          <div className="mt-1 text-[10.5px] text-muted">
            {t("artifacts.inventoryDisclaimer", undefined, "Historical checks are not current-file verification.")}
          </div>
        </>
      )}
    </div>
  );
}

export function ArtifactReviewMark({ status }: { status: ArtifactReviewStatus }) {
  const { t } = useI18n();
  const label = status === "passed"
    ? t("artifacts.inventoryPassed", undefined, "Recorded pass")
    : status === "failed"
      ? t("artifacts.inventoryFailed", undefined, "Recorded failure")
      : t("artifacts.inventoryMissing", undefined, "No record");
  return (
    <span
      data-testid="artifact-review-status"
      data-state={status}
      className={"text-[10.5px] shrink-0 " + (status === "failed" ? "text-danger" : "text-muted")}
    >{label}</span>
  );
}
