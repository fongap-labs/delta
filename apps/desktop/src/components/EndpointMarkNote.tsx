import { useEffect, useState } from "react";
import { getEgressSettings, setEndpointMark, type EgressSettings } from "../api";
import { useI18n } from "@delta/i18n/I18nContext";
import { useConfirmedChange } from "./PrivacySection";

/** The host an endpoint URL names, or null when it names none. Marks are keyed by host. */
export function endpointHost(url: string): string | null {
  try {
    return new URL(url.trim()).hostname || null;
  } catch {
    return null;
  }
}

/**
 * The endpoint mark on the card that owns this URL. Shown only when the URL's host carries a mark,
 * and removable from here, so the card is where the user sees what they decided about it.
 */
export function EndpointMarkNote({ url }: { url: string }) {
  const { t } = useI18n();
  const host = endpointHost(url);
  const [settings, setSettings] = useState<EgressSettings | null>(null);
  useEffect(() => {
    if (!host) return;
    getEgressSettings()
      .then(setSettings)
      .catch(() => setSettings(null));
  }, [host]);
  const { run, dialog, error } = useConfirmedChange(setSettings);
  const entry = host ? settings?.endpoint_marks.find((mark) => mark.host === host) : undefined;
  if (!host || !entry) return null;
  return (
    <div className="flex flex-wrap items-center gap-2 mt-1.5 text-[12px] text-muted" data-testid="endpoint-mark">
      {dialog}
      <span>{t("egress.card.marked", { host, mark: t(`egress.mark.${entry.mark}`) })}</span>
      <button
        className="text-[12px] text-danger hover:underline"
        onClick={() => void run((isConfirmed) => setEndpointMark(host, null, isConfirmed))}
      >
        {t("egress.endpoints.remove")}
      </button>
      {error && <span className="text-danger">{error}</span>}
    </div>
  );
}
