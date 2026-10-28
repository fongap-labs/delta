import { useEffect, useState } from "react";
import {
  getPictureSettings,
  setPictureSettings,
  type PictureModelRow,
  type PictureSettings,
} from "../api";
import { LoadError } from "./ViewStates";
import { useI18n } from "@delta/i18n/I18nContext";

// Settings ▸ Models ▸ Pictures. Delta works out by itself which models read pictures (what the
// provider declares, the model's name, how a request went); this card shows the answer and where it
// comes from, lets the user correct it, and sets the vision model that describes pictures for the
// models that cannot read them.
const CARD = "rounded-xl2 border border-line bg-panel";
const FIELD_LABEL = "text-[12.5px] font-medium text-ink";
const FIELD_HELP = "text-[12px] text-muted mt-1.5 leading-relaxed";
const SELECT =
  "px-2 py-1.5 rounded-lg border border-line bg-bg text-[13px] text-ink outline-none focus:border-primary";

type Choice = "auto" | "yes" | "no";

const choiceOf = (row: PictureModelRow): Choice =>
  row.marked === null ? "auto" : row.marked ? "yes" : "no";

export function PicturesCard() {
  const { t } = useI18n();
  const [settings, setSettings] = useState<PictureSettings | null>(null);
  const [hasFailed, setFailed] = useState(false);
  const [saveError, setSaveError] = useState("");

  const load = () => {
    setFailed(false);
    getPictureSettings().then(setSettings).catch(() => setFailed(true));
  };
  useEffect(load, []);

  const save = async (patch: Parameters<typeof setPictureSettings>[0]) => {
    setSaveError("");
    const result = await setPictureSettings(patch).catch((error) => ({
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    }));
    if (!result.ok) setSaveError(result.error || "unknown");
    load();
  };

  if (hasFailed && !settings)
    return <LoadError what={t("state.what.settings", undefined, "these settings")} onRetry={load} />;
  if (!settings) return null;

  const supportText = (row: PictureModelRow) =>
    row.support === "yes"
      ? t("settings.pictures.yes", undefined, "Reads pictures")
      : row.support === "no"
        ? t("settings.pictures.no", undefined, "Does not read pictures")
        : t("settings.pictures.unknown", undefined, "Not known yet");
  const sourceText = (row: PictureModelRow) =>
    row.source === "manual"
      ? t("settings.pictures.source.manual", undefined, "set by you")
      : row.source === "learned"
        ? t("settings.pictures.source.learned", undefined, "learned from a request")
        : row.source === "declared"
          ? t("settings.pictures.source.declared", undefined, "the provider says so")
          : row.source === "name"
            ? t("settings.pictures.source.name", undefined, "from the model’s name")
            : t(
                "settings.pictures.unknownHelp",
                undefined,
                "Pictures are sent to it; if it refuses them, Delta sends words instead and remembers.",
              );

  return (
    <div className={CARD + " p-4 mb-4"} data-testid="pictures-card">
      <div className={FIELD_LABEL}>{t("settings.pictures.title", undefined, "Pictures")}</div>
      <div className={FIELD_HELP}>
        {t(
          "settings.pictures.help",
          undefined,
          "Some models cannot read pictures. Delta works out which ones by itself — from what the provider says, from the model’s name and from how a request goes — and you can correct it here.",
        )}
      </div>

      <label className="mt-3 flex items-center gap-2.5">
        <span className="text-[13px] text-ink">{t("settings.pictures.visionModel", undefined, "Vision model")}</span>
        <select
          className={SELECT}
          value={settings.vision_model}
          data-testid="pictures-vision-model"
          onChange={(event) => void save({ vision_model: event.target.value })}
        >
          <option value="">{t("settings.pictures.none", undefined, "None")}</option>
          {settings.vision_model && !settings.models.some((row) => row.model === settings.vision_model) && (
            <option value={settings.vision_model}>{settings.vision_model}</option>
          )}
          {settings.models.map((row) => (
            <option key={row.model} value={row.model}>
              {row.model}
            </option>
          ))}
        </select>
      </label>
      <div className={FIELD_HELP}>
        {t(
          "settings.pictures.visionHelp",
          undefined,
          "For a model that cannot read pictures, this model describes each picture first and the description goes into the chat. Leave it empty to send no picture to such a model.",
        )}
      </div>

      <div className="mt-4 text-[13px] text-ink">{t("settings.pictures.models", undefined, "Which models read pictures")}</div>
      {settings.models.length === 0 ? (
        <div className={FIELD_HELP}>{t("settings.pictures.empty", undefined, "No models yet.")}</div>
      ) : (
        <div className="mt-2 divide-y divide-line" data-testid="pictures-models">
          {settings.models.map((row) => (
            <div
              key={row.model}
              className="py-2 flex items-center gap-3"
              data-testid={`pictures-row-${row.model}`}
            >
              <div className="min-w-0 flex-1">
                <div className="text-[13px] text-ink truncate">{row.model}</div>
                <div className="text-[12px] text-muted" data-testid={`pictures-status-${row.model}`}>
                  {supportText(row)} · {sourceText(row)}
                </div>
              </div>
              <select
                className={SELECT}
                value={choiceOf(row)}
                aria-label={row.model}
                data-testid={`pictures-choice-${row.model}`}
                onChange={(event) => {
                  const choice = event.target.value as Choice;
                  void save({
                    vision_support: { [row.model]: choice === "auto" ? null : choice === "yes" },
                  });
                }}
              >
                <option value="auto">{t("settings.pictures.auto", undefined, "Auto")}</option>
                <option value="yes">{t("settings.pictures.yes", undefined, "Reads pictures")}</option>
                <option value="no">{t("settings.pictures.no", undefined, "Does not read pictures")}</option>
              </select>
            </div>
          ))}
        </div>
      )}
      {saveError && (
        <div className="text-[12.5px] text-danger mt-2" data-testid="pictures-error">
          {t("settings.pictures.saveFailed", { error: saveError }, `Couldn’t save: ${saveError}`)}
        </div>
      )}
    </div>
  );
}
