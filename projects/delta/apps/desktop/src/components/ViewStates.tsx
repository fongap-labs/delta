/**
 * The two states every data view shares besides "empty": still loading, and could not load.
 *
 * "Could not load" must never look like "nothing here": a failed request used to clear the list
 * and show the empty message, which reads as "you have no records". These components keep the
 * three apart — loading is a quiet skeleton, a failure says so and offers a retry.
 */
import { useEffect, useState } from "react";
import { useI18n } from "@delta/i18n/I18nContext";
import { Icon } from "./Icon";

/** True once `isActive` has stayed true for `delayMs`, so fast loads never flash a skeleton. */
export function useAfterDelay(isActive: boolean, delayMs = 400): boolean {
  const [isElapsed, setElapsed] = useState(false);
  useEffect(() => {
    if (!isActive) {
      setElapsed(false);
      return;
    }
    const timer = window.setTimeout(() => setElapsed(true), delayMs);
    return () => window.clearTimeout(timer);
  }, [isActive, delayMs]);
  return isElapsed;
}

/** Same-shape placeholder rows in the secondary fill; they breathe, and hold still under reduced motion. */
export function LoadingRows({ count = 3 }: { count?: number }) {
  const { t } = useI18n();
  const isVisible = useAfterDelay(true);
  return (
    <div role="status" aria-busy="true" aria-label={t("common.loading", undefined, "Loading…")} data-testid="loading-rows">
      {isVisible && (
        <div className="space-y-2" aria-hidden="true">
          {Array.from({ length: count }, (_, index) => (
            <div key={index} className="skeleton-row rounded-xl2 border border-line bg-panel p-3.5 flex items-center gap-3" style={{ opacity: 1 - index * 0.2 }}>
              <span className="w-9 h-9 rounded-[10px] bg-surface-2 shrink-0" />
              <span className="flex-1 flex flex-col gap-2">
                <span className="h-3 w-[55%] rounded-md bg-surface-2" />
                <span className="h-2.5 w-[35%] rounded-md bg-surface-2" />
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

/** Says what happened and what to do; the retry runs the same request again. */
export function LoadError({ onRetry, what }: { onRetry: () => void; what?: string }) {
  const { t } = useI18n();
  return (
    <div
      role="alert"
      data-testid="load-error"
      className="rounded-xl2 border border-danger-line bg-danger-soft p-4 flex items-start gap-3"
    >
      <span className="text-danger mt-0.5 shrink-0">
        <Icon name="failCircle" size={18} />
      </span>
      <div className="flex-1 min-w-0">
        <div className="text-[14px] font-semibold text-danger">
          {what ? t("state.loadFailedWhat", { what }, `Couldn't load ${what}`) : t("state.loadFailed", undefined, "Couldn't load this")}
        </div>
        <div className="text-[13px] text-ink mt-0.5 leading-relaxed">
          {t("state.loadFailedHint", undefined, "Nothing was changed. Check that Delta is running, then try again.")}
        </div>
        <button
          type="button"
          className="mt-2.5 h-8 px-3 rounded-[10px] border border-line-strong bg-panel text-[13px] font-medium cursor-pointer hover:bg-surface-2"
          onClick={onRetry}
        >
          {t("common.retry", undefined, "Retry")}
        </button>
      </div>
    </div>
  );
}
