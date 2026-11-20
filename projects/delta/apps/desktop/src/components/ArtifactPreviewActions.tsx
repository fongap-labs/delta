import type { ArtifactInfo } from "../api";
import { revealArtifact } from "../api";
import { useI18n } from "@delta/i18n/I18nContext";
import { IconButton } from "./ui/IconButton";

interface ActionsProps {
  sessionId: string;
  artifact: ArtifactInfo;
  previewKind?: string;
  loading: boolean;
  onReload: () => Promise<void>;
  onReloaded: () => void;
}

/** Shared actions across every artifact renderer. The runtime remains the authority for OS opens. */
export function ArtifactPreviewActions({
  sessionId, artifact, previewKind, loading, onReload, onReloaded,
}: ActionsProps) {
  const { t } = useI18n();
  const canOpen = previewKind === "sheet" || previewKind === "pdf" || previewKind === "office";
  return (
    <div className="rail-actions" role="toolbar" aria-label={t("artifacts.previewActions", undefined, "Preview actions")}>
      <IconButton
        icon="refresh"
        className="artifact-icon-btn"
        disabled={loading}
        onClick={() => { void onReload().then(onReloaded); }}
        label={t("artifacts.reloadPreview")}
      />
      {canOpen && (
        <IconButton
          icon="panelOpen"
          className="artifact-icon-btn"
          onClick={() => { void revealArtifact(sessionId, artifact.path, "open"); }}
          label={t("artifacts.openInDefaultApp")}
        />
      )}
      <IconButton
        icon="copy"
        className="artifact-icon-btn"
        onClick={() => { void navigator.clipboard?.writeText(artifact.abs_path || artifact.path); }}
        label={t("artifacts.copyPath")}
        title={t("artifacts.copyFullPath")}
      />
      <IconButton
        icon="folder"
        className="artifact-icon-btn"
        onClick={() => { void revealArtifact(sessionId, artifact.path, "reveal"); }}
        label={t("artifacts.showInFolder")}
      />
    </div>
  );
}

interface FactsProps {
  artifact: ArtifactInfo;
  /** "loaded" means the conversation's recorded checks were inspected successfully. */
  checksLookup: "loading" | "loaded" | "unavailable";
  hasRecordedChecks: boolean;
  /** The delivery review can own missing-check messaging without repeating it. */
  showMissingChecks?: boolean;
}

/** Only runtime-provided facts; never invent a file version or a passed validation. */
export function ArtifactPreviewFacts({ artifact, checksLookup, hasRecordedChecks, showMissingChecks = true }: FactsProps) {
  const { t } = useI18n();
  // Deep links use a minimal 0/0 placeholder; don't mistake that for real file metadata.
  const hasTimestamp = Number.isFinite(artifact.modified_at) && artifact.modified_at > 0;
  const date = hasTimestamp ? new Date(artifact.modified_at * 1000) : null;
  const validDate = date && Number.isFinite(date.getTime()) ? date : null;
  const knownSize = validDate && Number.isFinite(artifact.size) && artifact.size >= 0;
  if (!validDate && checksLookup !== "loaded") return null;

  const bytes = artifact.size;
  const size = bytes < 1024 ? bytes + " B"
    : bytes < 1024 * 1024 ? (bytes / 1024).toFixed(1) + " KB"
    : (bytes / (1024 * 1024)).toFixed(1) + " MB";
  return (
    <div className="flex flex-wrap gap-x-2 gap-y-1 text-[11px] text-muted" data-testid="artifact-preview-facts">
      {knownSize && <span data-testid="artifact-file-size">{size}</span>}
      {validDate && (
        <span>
          {t("artifacts.modified", undefined, "Modified")}{" "}
          <time dateTime={validDate.toISOString()} data-testid="artifact-modified-at">
            {validDate.toLocaleDateString()}
          </time>
        </span>
      )}
      {showMissingChecks && checksLookup === "loaded" && !hasRecordedChecks && (
        <span data-testid="artifact-checks-unreported">
          {t("artifacts.checksUnreported", undefined, "No validation checks recorded")}
        </span>
      )}
    </div>
  );
}
