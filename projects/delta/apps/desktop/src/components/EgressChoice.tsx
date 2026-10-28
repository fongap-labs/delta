import { useState } from "react";
import { setEgressDefault, type EgressLevel } from "../api";
import { useI18n } from "@delta/i18n/I18nContext";
import { EgressLevelList } from "./PrivacySection";

/** The first-run (and first-after-upgrade) choice of the default egress level (ADR-0057). Nothing
 *  is preselected and the runtime sends nothing to a model until a level is chosen here. */
export function EgressChoice({ onChosen }: { onChosen: () => void }) {
  const { t } = useI18n();
  const [level, setLevel] = useState<EgressLevel | null>(null);
  const [isSaving, setSaving] = useState(false);
  const [error, setError] = useState("");

  const choose = async () => {
    if (!level) return;
    setSaving(true);
    setError("");
    // The first choice is never a loosening, so it needs no confirmation.
    const out = await setEgressDefault(level, false).catch(() => null);
    setSaving(false);
    if (!out || !out.ok) {
      setError(t("egress.error.save"));
      return;
    }
    onChosen();
  };

  return (
    <div className="gate-overlay" role="dialog" aria-modal="true" aria-labelledby="egress-choice-title">
      <div className="gate max-w-[560px]" data-testid="egress-choice">
        <div className="gate-mark">✦</div>
        <h2 id="egress-choice-title">{t("egress.choice.title")}</h2>
        <p className="gate-sub">{t("egress.choice.sub")}</p>
        <EgressLevelList name="egress-choice" value={level} onChange={setLevel} />
        {error && <div className="gate-error">{error}</div>}
        <div className="gate-foot justify-end gap-2">
          <button className="btn primary" disabled={!level || isSaving} onClick={() => void choose()}>
            {isSaving ? t("egress.choice.saving") : t("egress.choice.confirm")}
          </button>
        </div>
      </div>
    </div>
  );
}
