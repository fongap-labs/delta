import { useEffect, useState } from "react";
import { getWebhookSettings, saveWebhookSettings, testWebhook, type WebhookSettings, type WebhookTest } from "../api";
import { useI18n } from "@delta/i18n/I18nContext";

const CARD = "rounded-xl2 border border-line bg-panel p-4 mb-4";
const BTN = "text-[12.5px] px-3 py-1.5 rounded-lg border border-line bg-bg hover:border-line-strong shrink-0";

/** The optional completion webhook: one URL that gets a short post when a turn finishes. Whether a
 *  post may go is decided by the workspace's egress level each time, not by this card. */
export function WebhookCard() {
  const { t } = useI18n();
  const [saved, setSaved] = useState<WebhookSettings | null>(null);
  const [url, setUrl] = useState("");
  const [isEnabled, setEnabled] = useState(false);
  const [isDetailed, setDetailed] = useState(false);
  const [message, setMessage] = useState("");
  const [tested, setTested] = useState<WebhookTest | null>(null);
  const [isBusy, setBusy] = useState(false);

  const show = (settings: WebhookSettings) => {
    setSaved(settings);
    setUrl(settings.url ?? "");
    setEnabled(settings.is_enabled);
    setDetailed(settings.include_details);
  };

  useEffect(() => {
    getWebhookSettings()
      .then(show)
      .catch(() => setMessage(t("webhook.error.load")));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const isDirty =
    saved !== null &&
    (url.trim() !== (saved.url ?? "") || isEnabled !== saved.is_enabled || isDetailed !== saved.include_details);

  const save = async () => {
    setMessage("");
    setTested(null);
    try {
      show(await saveWebhookSettings({ url: url.trim() === "" ? null : url.trim(), is_enabled: isEnabled, include_details: isDetailed }));
      setMessage(t("webhook.saved"));
    } catch (error) {
      setMessage(t("webhook.error.save", { error: error instanceof Error ? error.message : String(error) }));
    }
  };

  const test = async () => {
    setMessage("");
    setTested(null);
    setBusy(true);
    try {
      setTested(await testWebhook());
    } catch (error) {
      setMessage(t("webhook.error.test", { error: error instanceof Error ? error.message : String(error) }));
    } finally {
      setBusy(false);
    }
  };

  const outcome = !tested
    ? ""
    : tested.result === "sent"
      ? t("webhook.result.sent", { host: tested.host, status: tested.status })
      : tested.result === "skipped"
        ? t("webhook.result.skipped", { host: tested.host, reason: tested.reason })
        : tested.result === "failed"
          ? t("webhook.result.failed", { host: tested.host, error: tested.error })
          : t("webhook.result.off");

  return (
    <section className={CARD} data-testid="webhook-card">
      <div className="text-[13.5px] font-semibold">{t("webhook.title")}</div>
      <div className="text-[12px] text-muted mt-1 mb-3">{t("webhook.sub")}</div>
      <input
        className="w-full px-3 py-1.5 rounded-lg border border-line bg-bg text-[12.5px] outline-none focus:border-primary mb-2"
        placeholder={t("webhook.urlPlaceholder")}
        value={url}
        data-testid="webhook-url"
        onChange={(e) => setUrl(e.target.value)}
      />
      <label className="flex items-center gap-2 py-1">
        <input type="checkbox" checked={isEnabled} data-testid="webhook-enabled" onChange={(e) => setEnabled(e.target.checked)} />
        <span className="text-[12.5px]">{t("webhook.enable")}</span>
      </label>
      <label className="flex items-start gap-2 py-1">
        <input
          type="checkbox"
          className="mt-0.5"
          checked={isDetailed}
          data-testid="webhook-details"
          onChange={(e) => setDetailed(e.target.checked)}
        />
        <span>
          <span className="block text-[12.5px]">{t("webhook.details")}</span>
          <span className="block text-[12px] text-muted">{t("webhook.detailsHelp")}</span>
        </span>
      </label>
      <div className="flex items-center gap-2 mt-3 flex-wrap">
        <button className={BTN} disabled={!isDirty} data-testid="webhook-save" onClick={() => void save()}>
          {t("webhook.save")}
        </button>
        <button
          className={BTN}
          disabled={isBusy || isDirty || !saved?.url}
          data-testid="webhook-test"
          onClick={() => void test()}
        >
          {isBusy ? t("webhook.testing") : t("webhook.test")}
        </button>
        <span className="text-[12px] text-muted" role="status" data-testid="webhook-message">
          {outcome || message}
        </span>
      </div>
      <div className="text-[12px] text-muted mt-2">{t("webhook.testHelp")}</div>
    </section>
  );
}
