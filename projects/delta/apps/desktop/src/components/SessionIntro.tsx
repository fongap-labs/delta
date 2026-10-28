import { useState } from "react";
import { useI18n } from "@delta/i18n/I18nContext";
import { Icon, type IconName } from "./Icon";

// Empty-state for a fresh session: an illustration, the greeting, then a guided choice of starting
// points for the three kinds of work Delta is for (office, research, content). Picking a kind
// shows examples of it; an example only fills the composer. Nothing runs until the user sends it,
// and they can edit it first.

const KINDS = [
  { key: "office", icon: "file", examples: ["files", "report", "minutes"] },
  { key: "research", icon: "search", examples: ["pdf", "compare", "folder"] },
  { key: "create", icon: "book", examples: ["topics", "article", "deck"] },
] as const satisfies ReadonlyArray<{ key: string; icon: IconName; examples: readonly string[] }>;

type Kind = (typeof KINDS)[number]["key"];

// A workbench with three work items fanned out — a document, a chart and a checklist — under the
// Delta sparkle. Drawn with theme tokens only (see `.intro-art` in styles.css), so it follows the
// light and dark themes.
function IntroArt() {
  return (
    <svg className="intro-art" viewBox="0 0 200 124" role="img" aria-hidden="true" data-testid="intro-art">
      <ellipse className="art-shadow" cx="100" cy="114" rx="72" ry="5" />
      <g transform="rotate(-8 54 66)">
        <rect className="art-card" x="22" y="28" width="64" height="80" rx="8" />
        <rect className="art-line" x="32" y="42" width="30" height="4" rx="2" />
        <rect className="art-line" x="32" y="54" width="44" height="3" rx="1.5" />
        <rect className="art-line" x="32" y="63" width="40" height="3" rx="1.5" />
        <rect className="art-line" x="32" y="72" width="44" height="3" rx="1.5" />
        <rect className="art-line" x="32" y="81" width="26" height="3" rx="1.5" />
      </g>
      <g transform="rotate(7 146 66)">
        <rect className="art-card" x="114" y="26" width="64" height="80" rx="8" />
        <rect className="art-bar art-bar-soft" x="126" y="72" width="9" height="22" rx="2" />
        <rect className="art-bar art-bar-mid" x="141" y="58" width="9" height="36" rx="2" />
        <rect className="art-bar" x="156" y="44" width="9" height="50" rx="2" />
      </g>
      <rect className="art-card art-card-front" x="64" y="14" width="72" height="94" rx="10" />
      {[0, 1, 2].map((row) => (
        <g key={row}>
          <circle className={row < 2 ? "art-dot art-dot-done" : "art-dot"} cx="80" cy={44 + row * 22} r="6" />
          {row < 2 && <path className="art-tick" d={`m77.2 ${44 + row * 22} 2.2 2.2 3.8-4.4`} />}
          <rect className="art-line" x="92" y={42 + row * 22} width={row === 2 ? 26 : 34} height="4" rx="2" />
        </g>
      ))}
      <circle className="art-badge" cx="132" cy="18" r="12" />
      <path
        className="art-spark"
        d="M132 10.6c.4 3.9 2.1 5.6 6 6-3.9.4-5.6 2.1-6 6-.4-3.9-2.1-5.6-6-6 3.9-.4 5.6-2.1 6-6z"
      />
    </svg>
  );
}

export function SessionIntro({ onPick }: { onPick?: (text: string) => void }) {
  const { t } = useI18n();
  const [kind, setKind] = useState<Kind>("office");
  const shown = KINDS.find((entry) => entry.key === kind) ?? KINDS[0];
  return (
    <div className="intro">
      <IntroArt />
      <h1 className="greeting">{t("sessionIntro.greeting")}</h1>
      <p className="intro-lede">{t("sessionIntro.lede")}</p>
      {onPick && (
        <>
          <div className="intro-kinds" role="tablist" aria-label={t("sessionIntro.kindsLabel")} data-testid="intro-kinds">
            {KINDS.map((entry) => (
              <button
                key={entry.key}
                role="tab"
                aria-selected={entry.key === kind}
                className={"intro-kind" + (entry.key === kind ? " is-active" : "")}
                data-testid={`intro-kind-${entry.key}`}
                onClick={() => setKind(entry.key)}
              >
                <Icon name={entry.icon} size={14} />
                <span>{t(`sessionIntro.kind.${entry.key}`)}</span>
              </button>
            ))}
          </div>
          <div className="intro-examples" role="tabpanel" data-testid="intro-examples">
            {shown.examples.map((example) => {
              const text = t(`sessionIntro.example.${example}`);
              return (
                <button key={example} className="intro-example" data-testid="intro-example" onClick={() => onPick(text)}>
                  {text}
                </button>
              );
            })}
          </div>
        </>
      )}
    </div>
  );
}
