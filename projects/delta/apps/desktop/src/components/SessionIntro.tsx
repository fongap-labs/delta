import { useI18n } from "@delta/i18n/I18nContext";

// Empty-state for a fresh session: the greeting, then a few starting points for the three kinds
// of work Delta is for (office, research, content). A starting point only fills the composer;
// nothing runs until the user sends it, and they can edit it first.

const EXAMPLES = ["files", "pdf", "topics", "report"] as const;

export function SessionIntro({ onPick }: { onPick?: (text: string) => void }) {
  const { t } = useI18n();
  return (
    <div className="intro">
      <h1 className="greeting">{t("sessionIntro.greeting")}</h1>
      {onPick && (
        <div className="intro-examples" data-testid="intro-examples">
          {EXAMPLES.map((key) => {
            const text = t(`sessionIntro.example.${key}`);
            return (
              <button key={key} className="intro-example" data-testid="intro-example" onClick={() => onPick(text)}>
                {text}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
