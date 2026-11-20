import { Button } from "./Button";
import { useI18n } from "@delta/i18n/I18nContext";

interface Props {
  state: "loading" | "error";
  message: string;
  onRetry?: () => void;
}

/** Shared loading and failure surface for artifact previews, including PDF rendering. */
export function PreviewFeedback({ state, message, onRetry }: Props) {
  const { t } = useI18n();
  return (
    <div
      role={state === "error" ? "alert" : "status"}
      data-testid="artifact-preview-feedback"
      className={state === "error" ? "rail-error artifact-table-note" : "rail-muted artifact-table-note"}
    >
      <span>{message}</span>
      {state === "error" && onRetry && (
        <Button size="sm" className="ml-2" onClick={onRetry}>
          {t("artifacts.retryPreview", undefined, "Retry preview")}
        </Button>
      )}
    </div>
  );
}
