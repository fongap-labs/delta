import { useEffect, useRef, useState } from "react";
import {
  acceptMemoryCandidate,
  announceMemoryChanged,
  deleteMemory,
  getMemoryCandidates,
  rejectMemoryCandidate,
} from "../api";
import { useI18n } from "@delta/i18n/I18nContext";
import { Button } from "./ui/Button";

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
  const [isSaving, setSaving] = useState(false);
  const requestPending = useRef(false);

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

  const runDecision = async (action: () => Promise<boolean>, success: Phase) => {
    if (requestPending.current) return;
    requestPending.current = true;
    setSaving(true);
    try {
      const ok = await action();
      if (!ok) {
        setPhase("failed");
        return;
      }
      setPhase(success);
      announceMemoryChanged();
    } catch {
      setPhase("failed");
    } finally {
      requestPending.current = false;
      setSaving(false);
    }
  };

  const accept = async (content?: string) => {
    await runDecision(async () => {
      const result = await acceptMemoryCandidate(candidateId, content);
      if (!result.ok) return false;
      if (content) setShown(content);
      setSavedId(result.memory_id ?? null);
      return true;
    }, "saved");
  };
  const decline = async () =>
    runDecision(async () => (await rejectMemoryCandidate(candidateId)).ok, "declined");
  const undo = async () =>
    runDecision(async () => savedId !== null && (await deleteMemory(savedId)).ok, "undone");

  if (phase === "checking" || phase === "gone") return null;

  return (
    <div className="notice text-left" data-testid="memory-suggestion">
      {phase === "failed" && (
        <span role="alert" data-testid="memory-suggestion-failed">
          {t("memorySuggestion.failed", undefined, "Couldn’t update that suggestion. You can still decide in Settings ▸ Memory.")}
        </span>
      )}
      {phase === "declined" && (
        <span role="status" data-testid="memory-suggestion-declined">
          {t("memorySuggestion.declined", undefined, "Okay — I won’t remember that.")}
        </span>
      )}
      {phase === "undone" && (
        <span role="status" data-testid="memory-suggestion-undone">
          {t("memorySuggestion.undone", undefined, "Okay — forgotten.")}
        </span>
      )}
      {phase === "saved" && (
        <div className="flex items-center gap-2">
          <span className="min-w-0">
            <span className="font-medium">{t("memorySuggestion.saved", undefined, "Remembered")}</span>
            <span className="text-muted"> — {shown}</span>
          </span>
          <Button size="sm" className="ml-auto shrink-0" disabled={isSaving} aria-busy={isSaving}
            data-testid="memory-suggestion-undo" onClick={() => void undo()}>
            {t("transcript.undo", undefined, "Undo")}
          </Button>
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
            <Button variant="primary" size="sm" disabled={isSaving} aria-busy={isSaving}
              data-testid="memory-suggestion-accept" onClick={() => void accept()}>
              {t("memorySuggestion.accept", undefined, "Remember")}
            </Button>
            <Button
              size="sm"
              disabled={isSaving}
              data-testid="memory-suggestion-edit"
              onClick={() => {
                setDraft(shown);
                setPhase("editing");
              }}
            >
              {t("memorySuggestion.edit", undefined, "Edit")}
            </Button>
            <Button variant="ghost" size="sm" disabled={isSaving} data-testid="memory-suggestion-decline"
              onClick={() => void decline()}>
              {t("memorySuggestion.decline", undefined, "No thanks")}
            </Button>
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
            aria-label={t("memorySuggestion.edit", undefined, "Edit")}
            className="ds-field w-full text-[13px] resize-y leading-relaxed"
          />
          <div className="flex items-center gap-2 mt-1.5">
            <Button variant="primary" size="sm" data-testid="memory-suggestion-save"
              disabled={isSaving || !draft.trim()} aria-busy={isSaving}
              onClick={() => void accept(draft.trim())}>
              {t("memorySuggestion.save", undefined, "Save")}
            </Button>
            <Button size="sm" disabled={isSaving} onClick={() => setPhase("pending")}>
              {t("common.cancel", undefined, "Cancel")}
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
