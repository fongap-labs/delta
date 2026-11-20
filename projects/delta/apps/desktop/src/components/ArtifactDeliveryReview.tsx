import type { ArtifactTrust } from "../artifactTrust";
import { summarizeChecks } from "../artifactTrust";
import { useI18n } from "@delta/i18n/I18nContext";
import { useState } from "react";
import { DisclosureSection } from "./ui/DisclosureSection";
import { ArtifactIntegrityAction } from "./ArtifactIntegrityAction";

type Lookup = "loading" | "loaded" | "unavailable";
type Preview = "loading" | "available" | "unavailable";

interface Props {
  preview: Preview;
  checksLookup: Lookup;
  trust: ArtifactTrust | null;
  sessionId?: string;
  path?: string;
}

/**
 * Read-only delivery evidence. A recorded historical verdict is NOT a check of
 * the current file's bytes and cannot authorize distribution or approval.
 */
export function ArtifactDeliveryReview({ preview, checksLookup, trust, sessionId, path }: Props) {
  const { t } = useI18n();
  const [isOpen, setOpen] = useState(false);
  const summary = trust ? summarizeChecks(trust) : null;
  const labels = {
    waiting: t("artifacts.reviewWaiting", undefined, "Checking…"),
    notAvailable: t("artifacts.reviewUnavailable", undefined, "Unavailable"),
    notRecorded: t("artifacts.reviewNotRecorded", undefined, "Not recorded"),
  };
  const previewLabel = preview === "loading" ? labels.waiting
    : preview === "available" ? t("artifacts.reviewReadable", undefined, "Preview loaded") : labels.notAvailable;
  const validationLabel = checksLookup === "loading" ? labels.waiting
    : checksLookup === "unavailable" ? labels.notAvailable
    : !summary ? labels.notRecorded
    : trust?.isValidationOk
      ? t("artifacts.reviewChecksPassed", { count: summary.passed, total: summary.total }, "Recorded checks passed: " + summary.passed + "/" + summary.total)
      : t("artifacts.reviewChecksFailed", { count: summary.total - summary.passed, total: summary.total }, "Recorded checks failed: " + (summary.total - summary.passed) + "/" + summary.total);
  const retainedLabel = checksLookup === "loading" ? labels.waiting
    : checksLookup === "unavailable" ? labels.notAvailable
    : trust?.previousVersion
      ? t("artifacts.reviewPreviousRecorded", undefined, "Earlier version path recorded")
      : labels.notRecorded;
  return (
    <div data-testid="artifact-delivery-review" className="px-[18px] py-2 border-b border-line">
      <DisclosureSection title={t("artifacts.reviewTitle", undefined, "Delivery evidence")} isOpen={isOpen} onToggle={() => setOpen((value) => !value)}>
        <dl className="grid gap-2 text-[12px]">
          <ReviewLine label={t("artifacts.reviewPreview", undefined, "Preview")} value={previewLabel} testId="artifact-review-preview" />
          <ReviewLine label={t("artifacts.reviewValidation", undefined, "Recorded validation")} value={validationLabel} testId="artifact-review-validation" />
          <ReviewLine label={t("artifacts.reviewPrevious", undefined, "Earlier version")} value={retainedLabel} testId="artifact-review-previous" />
        </dl>
        {trust?.previousVersion && checksLookup === "loaded" && (
          <div className="mt-1 text-muted break-all font-mono text-[11px]" data-testid="artifact-review-backup-path">{trust.previousVersion}</div>
        )}
        {sessionId && path && <ArtifactIntegrityAction key={sessionId + ":" + path} sessionId={sessionId} path={path} />}
        <p className="mt-2 text-muted text-[11px]" data-testid="artifact-review-disclaimer">
          {t("artifacts.reviewDisclaimer", undefined, "Recorded evidence is not a fresh validation of the current file or delivery approval.")}
        </p>
      </DisclosureSection>
    </div>
  );
}

function ReviewLine({ label, value, testId }: { label: string; value: string; testId: string }) {
  return (
    <div className="flex flex-wrap justify-between gap-x-3 gap-y-1">
      <dt className="text-muted">{label}</dt>
      <dd data-testid={testId} className="text-ink">{value}</dd>
    </div>
  );
}

