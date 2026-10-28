import { useEffect, useState } from "react";
import {
  acceptMemoryCandidate,
  announceMemoryChanged,
  deleteMemory,
  getMemoryCandidates,
  rejectMemoryCandidate,
} from "../api";
import { useI18n } from "@delta/i18n/I18nContext";

// A fact the model suggested remembering. Nothing is saved until the user says so: the card
// offers Remember / Edit / No thanks, and an accepted note can be undone in place. The same
// decisions can be made later in Settings ▸ Memory, so a card whose suggestion was already
// decided there (or in an earlier session) shows nothing instead of offering it twice.
type Phase = "checking" | "pending" | "editing" | "saved" | "declined" | "undone" | "gone" | "failed";

export function MemorySuggestion({
  candidateId,
  text,
  quote,
}: {
  candidateId: number;
  text: string;
  quote?: string;
}) {
  const { t } = useI18n();
  const [phase, setPhase] = useState<Phase>("checking");
  const [shown, setShown] = useState(text);
  const [draft, setDraft] = useState(text);
  const [savedId, setSavedId] = useState<number | null>(null);

  useEffect(() => {
    let isCurrent = true;
    getMemoryCandidates()
      .then((waiting) => {
        if (!isCurrent) return;
        const mine = waiting.find((candidate) => candidate.id === candidateId);
        if (mine) setShown(mine.content);
        setPhase(mine ? "pending" : "gone");
      })
      .catch(() => isCurrent && setPhase("failed"));
    return () => {
      isCurrent = false;
    };
  }, [candidateId]);

  const accept = async (content?: string) => {
    const result = await acceptMemoryCandidate(candidateId, content).catch(() => null);
    if (!result?.ok) {
      setPhase("failed");
      return;
    }
    if (content) setShown(content);
    setSavedId(result.memory_id ?? null);
    setPhase("saved");
    announceMemoryChanged();
  };
  const decline = async () => {
    const result = await rejectMemoryCandidate(candidateId).catch(() => null);
    setPhase(result?.ok ? "declined" : "failed");
    announceMemoryChanged();
  };
  const undo = async () => {
    if (savedId !== null) await deleteMemory(savedId).catch(() => {});
    setPhase("undone");
    announceMemoryChanged();
  };

  if (phase === "checking" || phase === "gone") return null;

  return (
    <div className="notice text-left" data-testid="memory-suggestion">
      {phase === "failed" && (
        <span data-testid="memory-suggestion-failed">
          {t("memorySuggestion.failed", undefined, "Couldn’t update that suggestion. You can still decide in Settings ▸ Memory.")}
        </span>
      )}
      {phase === "declined" && (
        <span data-testid="memory-suggestion-declined">
          {t("memorySuggestion.declined", undefined, "Okay — I won’t remember that.")}
        </span>
      )}
      {phase === "undone" && (
        <span data-testid="memory-suggestion-undone">
          {t("memorySuggestion.undone", undefined, "Okay — forgotten.")}
        </span>
      )}
      {phase === "saved" && (
        <div className="flex items-center gap-2">
          <span className="min-w-0">
            <span className="font-medium">{t("memorySuggestion.saved", undefined, "Remembered")}</span>
            <span className="text-muted"> — {shown}</span>
          </span>
          <button className="btn ml-auto shrink-0" data-testid="memory-suggestion-undo" onClick={() => void undo()}>
            {t("transcript.undo", undefined, "Undo")}
          </button>
        </div>
      )}
      {phase === "pending" && (
        <>
          <div>
            <span className="font-medium">
              {t("memorySuggestion.ask", undefined, "Remember this for next time?")}
            </span>
            <span className="text-muted"> — {shown}</span>
          </div>
          {quote ? (
            <div className="text-[12px] text-faint mt-0.5" data-testid="memory-suggestion-quote">
              {t("memorySuggestion.because", { quote }, `You said: “${quote}”`)}
            </div>
          ) : null}
          <div className="flex items-center gap-2 mt-1.5">
            <button className="btn" data-testid="memory-suggestion-accept" onClick={() => void accept()}>
              {t("memorySuggestion.accept", undefined, "Remember")}
            </button>
            <button
              className="btn"
              data-testid="memory-suggestion-edit"
              onClick={() => {
                setDraft(shown);
                setPhase("editing");
              }}
            >
              {t("memorySuggestion.edit", undefined, "Edit")}
            </button>
            <button className="btn" data-testid="memory-suggestion-decline" onClick={() => void decline()}>
              {t("memorySuggestion.decline", undefined, "No thanks")}
            </button>
          </div>
        </>
      )}
      {phase === "editing" && (
        <div data-testid="memory-suggestion-editor">
          <textarea
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            rows={2}
            autoFocus
            data-testid="memory-suggestion-input"
            className="w-full px-3 py-2 rounded-lg border border-line bg-bg text-[13px] text-ink outline-none focus:border-primary resize-y leading-relaxed"
          />
          <div className="flex items-center gap-2 mt-1.5">
            <button
              className="btn"
              data-testid="memory-suggestion-save"
              disabled={!draft.trim()}
              onClick={() => void accept(draft.trim())}
            >
              {t("memorySuggestion.save", undefined, "Save")}
            </button>
            <button className="btn" onClick={() => setPhase("pending")}>
              {t("common.cancel", undefined, "Cancel")}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
