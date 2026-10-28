import { useEffect, useState } from "react";
import { getRuntimePack, installRuntimePack, type RuntimePack } from "../api";
import { useI18n } from "@delta/i18n/I18nContext";

// The analysis runtime is a one-time download (ADR-0085). The user sees what will be fetched and
// from where, and confirms it; nothing is downloaded before that confirmation.
export function RuntimePackPrompt({ onInstalled }: { onInstalled: () => void }) {
  const { t } = useI18n();
  const [pack, setPack] = useState<RuntimePack | null>(null);
  const [isInstalling, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let isAlive = true;
    getRuntimePack()
      .then((status) => {
        if (isAlive) setPack(status);
      })
      .catch((failure) => {
        if (isAlive) setError(String(failure));
      });
    return () => {
      isAlive = false;
    };
  }, []);

  if (!pack || pack.installed) return null;

  const install = async () => {
    setInstalling(true);
    setError(null);
    const outcome = await installRuntimePack();
    setInstalling(false);
    if (outcome.installed) {
      setPack(outcome);
      onInstalled();
    } else {
      setError(outcome.error ?? t("runtimePack.failed", undefined, "The runtime could not be installed."));
    }
  };

  return (
    <section className="mt-4 rounded-lg border border-line p-3 text-[13px]">
      <p className="font-medium text-ink">{t("runtimePack.title", undefined, "Analysis runtime")}</p>
      {!pack.published ? (
        <p className="mt-1 text-muted">
          {t(
            "runtimePack.notPublished",
            undefined,
            "The runtime is not published yet, so it cannot be installed in this build."
          )}
        </p>
      ) : (
        <>
          <p className="mt-1 text-muted">
            {t(
              "runtimePack.describe",
              { name: pack.name ?? "", version: pack.version ?? "", mb: Math.ceil((pack.size ?? 0) / (1024 * 1024)), host: pack.host ?? "" },
              "Download {name} {version} (about {mb} MB) from {host}. It runs locally and is used only by the statistics and Office tools."
            )}
          </p>
          <button
            className="mt-3 rounded-lg bg-ink px-3 py-1.5 text-[13px] text-panel disabled:opacity-40"
            disabled={isInstalling}
            onClick={install}
          >
            {isInstalling
              ? t("runtimePack.installing", undefined, "Installing…")
              : t("runtimePack.confirm", undefined, "Download and install")}
          </button>
        </>
      )}
      {error && <p className="mt-2 text-[13px] text-danger">{error}</p>}
    </section>
  );
}
