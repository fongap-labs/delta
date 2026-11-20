import { useEffect, useRef, useState } from "react";
import {
  acceptMemoryCandidate,
  deleteAllMemory,
  deleteMemory,
  exportMemory,
  getMemory,
  getMemoryCandidates,
  getMemorySettings,
  importMemory,
  rejectMemoryCandidate,
  setMemorySettings,
  updateMemory,
  MEMORY_CHANGED,
  type MemoryCandidate,
  type MemoryEntry,
  type MemorySettings,
} from "../api";
import { Button } from "./ui/Button";
import { IconButton } from "./ui/IconButton";
import { PanelHead } from "./IntegrationsView";
import { LoadError, LoadingRows } from "./ViewStates";
import { Toggle } from "./Toggle";
import { useI18n } from "@delta/i18n/I18nContext";

// MEMORY-SPEC §5.3: the one memory screen. A plain-language list of remembered facts
// (edit/delete per row), the on/off toggle, delete-all, and the User Rules textarea —
// no scope vocabulary, no markdown, no files. Delta only SUGGESTS what to remember: a
// suggestion waits here (and in the conversation) until the user accepts, edits or declines it.
const CARD = "rounded-xl2 border border-line bg-panel";
const FIELD_LABEL = "text-[12.5px] font-medium text-ink";
const FIELD_HELP = "text-[12px] text-muted mt-1.5 leading-relaxed";

export function MemorySection() {
  const { t } = useI18n();
  const [settings, setSettings] = useState<MemorySettings | null>(null);
  const [entries, setEntries] = useState<MemoryEntry[] | null>(null);
  const [candidates, setCandidates] = useState<MemoryCandidate[]>([]);
  // State-change copy (§5.3): shown under the toggle / list after an action.
  const [toggleMsg, setToggleMsg] = useState<string | null>(null);
  const [listMsg, setListMsg] = useState<string | null>(null);
  // A failed read is not an empty memory: it shows an error with a retry instead of "Nothing yet".
  const [hasFailed, setFailed] = useState(false);
  const [hasListError, setHasListError] = useState(false);
  const [isWiping, setWiping] = useState(false);
  const wipePending = useRef(false);
  const importInput = useRef<HTMLInputElement>(null);

  const refresh = () => {
    setFailed(false);
    getMemorySettings().then(setSettings).catch(() => setFailed(true));
    getMemory().then(setEntries).catch(() => setFailed(true));
    getMemoryCandidates().then(setCandidates).catch(() => setCandidates([]));
  };
  useEffect(refresh, []);
  // Stay current while the screen is open: a save/edit landing in a conversation, or
  // the window regaining focus after one did. Without this the list is a snapshot from
  // whenever the page mounted — it showed "Nothing yet" seconds after a real save
  // (owner-hit 2026-07-28), which reads as "it didn't work".
  useEffect(() => {
    window.addEventListener(MEMORY_CHANGED, refresh);
    window.addEventListener("focus", refresh);
    return () => {
      window.removeEventListener(MEMORY_CHANGED, refresh);
      window.removeEventListener("focus", refresh);
    };
  }, []);

  const toggleEnabled = async () => {
    if (!settings) return;
    const next = await setMemorySettings({ enabled: !settings.enabled });
    setSettings(next);
    setToggleMsg(
      next.enabled
        ? t("memory.enabledMsg", undefined, "Delta may now suggest things to remember. Nothing is saved until you accept it.")
        : t("memory.disabledMsg", undefined, "Delta won’t suggest anything new. Notes you already saved stay here — delete any you’d rather not keep."),
    );
  };

  const wipeAll = async () => {
    if (wipePending.current) return;
    if (
      !window.confirm(
        t("memory.confirmDeleteAll", undefined, "Delete everything Delta has saved about you, including the earlier wording of corrected notes and suggestions still waiting?\n\nThis can’t be undone."),
      )
    )
      return;
    wipePending.current = true;
    setWiping(true);
    setHasListError(false);
    try {
      const result = await deleteAllMemory();
      if (!result.ok) throw new Error("delete failed");
      setListMsg(
        t("memory.deletedMsg", undefined, "Everything Delta had saved about you has been deleted."),
      );
      refresh();
    } catch {
      setHasListError(true);
      setListMsg(t("memory.deleteFailed", undefined, "Couldn't delete saved memories. Please try again."));
    } finally {
      wipePending.current = false;
      setWiping(false);
    }
  };

  // Export hands the user a `delta.memory` file; import reads one back. The runtime checks the
  // whole file and adds nothing when any entry is bad, so a wrong file cannot half-apply.
  const exportAll = async () => {
    setHasListError(false);
    try {
      const exported = await exportMemory();
      const url = URL.createObjectURL(
        new Blob([JSON.stringify(exported, null, 2)], { type: "application/json" }),
      );
      const link = window.document.createElement("a");
      link.href = url;
      link.download = "delta-memory.json";
      window.document.body.appendChild(link);
      link.click();
      link.remove();
      URL.revokeObjectURL(url);
    } catch (error) {
      const reason = error instanceof Error ? error.message : String(error);
      setHasListError(true);
      setListMsg(t("memory.exportFailed", { error: reason }, `Couldn't export your memories: ${reason}`));
    }
  };

  const importFile = async (file: File) => {
    setHasListError(false);
    try {
      const result = await importMemory(JSON.parse(await file.text()));
      if (!result.ok) throw new Error(result.error || "import failed");
      const imported = result.imported ?? 0;
      const skipped = result.skipped ?? 0;
      setListMsg(
        t("memory.importedMsg", { imported, skipped }, `Imported ${imported} memories; ${skipped} were already there.`),
      );
      refresh();
    } catch (error) {
      const reason = error instanceof Error ? error.message : String(error);
      setHasListError(true);
      setListMsg(t("memory.importFailed", { error: reason }, `Couldn't import that file: ${reason}`));
    }
  };

  if (hasFailed && (!settings || entries === null))
    return <LoadError what={t("state.what.memory", undefined, "memory")} onRetry={refresh} />;
  if (!settings || entries === null) return <LoadingRows count={2} />;

  return (
    <section>
      <PanelHead
        title={t("memory.title", undefined, "Memory")}
        sub={t("memory.sub", undefined, "Delta can suggest things worth remembering about you. Nothing is saved until you accept it, and saved notes aren’t sent to a model yet. You can keep, edit and delete notes here.")}
      />

      {/* On/off — one switch, no other setup (§5.4). */}
      <div className={CARD + " p-4 mb-4"} data-testid="memory-toggle-card">
        <div className="flex items-center gap-3">
          <Toggle checked={settings.enabled} onChange={() => void toggleEnabled()} title={t("memory.toggleTitle", undefined, "Remember new things about you")} />
          <div className="min-w-0 flex-1">
            <div className={FIELD_LABEL}>{t("memory.toggleLabel", undefined, "Remember new things about me")}</div>
            <div className="text-[12px] text-muted mt-0.5">
              {t("memory.toggleHelp", undefined, "When this is on, Delta may suggest remembering something you tell it. You decide each one; nothing is saved without you.")}
            </div>
          </div>
        </div>
        {toggleMsg && (
          <div className="text-[12.5px] text-muted mt-3 pt-3 border-t border-line" data-testid="memory-toggle-msg">
            {toggleMsg}
          </div>
        )}
      </div>

      {/* Suggestions waiting for a decision: nothing here is remembered yet. */}
      {candidates.length > 0 && (
        <div className={CARD + " p-4 mb-4"} data-testid="memory-suggestions-card">
          <div className={FIELD_LABEL}>{t("memory.suggestionsTitle", undefined, "Suggested by Delta")}</div>
          <div className={FIELD_HELP}>
            {t("memory.suggestionsHelp", undefined, "Things Delta thinks are worth remembering. Nothing is saved until you accept it.")}
          </div>
          <div className="mt-3 divide-y divide-line">
            {candidates.map((candidate) => (
              <CandidateRow key={candidate.id} candidate={candidate} onChanged={refresh} />
            ))}
          </div>
        </div>
      )}

      {/* What I've learned (§5.3): directly under the toggle that governs it — the off
          message ("what I already know is kept, delete it below") points here. */}
      <div className={CARD + " p-4 mb-4"} data-testid="memory-list-card">
        <div className="flex items-center gap-2">
          <div className={FIELD_LABEL + " flex-1"}>{t("memory.listTitle", undefined, "Notes about you")}</div>
          {entries.length > 0 && (
            <Button variant="ghost" size="sm" data-testid="memory-export" onClick={() => void exportAll()}>
              {t("memory.export", undefined, "Export")}
            </Button>
          )}
          <Button variant="ghost" size="sm" data-testid="memory-import" onClick={() => importInput.current?.click()}>
            {t("memory.import", undefined, "Import")}
          </Button>
          <input
            ref={importInput}
            type="file"
            accept=".json,application/json"
            className="hidden"
            data-testid="memory-import-input"
            onChange={(event) => {
              const file = event.target.files?.[0];
              event.target.value = "";
              if (file) void importFile(file);
            }}
          />
          {entries.length > 0 && (
            <Button variant="danger" size="sm" data-testid="memory-delete-all"
              disabled={isWiping} aria-busy={isWiping} onClick={() => void wipeAll()}>
              {t("memory.forgetAll", undefined, "Forget everything…")}
            </Button>
          )}
        </div>
        <div className={FIELD_HELP}>
          {t("memory.listHelp", undefined, "These notes are stored only on this computer, and Delta doesn’t use them in conversations yet. Fixing a note keeps its earlier wording as history here; deleting a note erases it and its earlier wording.")}
        </div>
        {listMsg && (
          <div role={hasListError ? "alert" : "status"} className="text-[12.5px] text-muted mt-2.5" data-testid="memory-list-msg">
            {listMsg}
          </div>
        )}
        {entries.length === 0 ? (
          !listMsg && (
            <div className="text-[12px] text-muted mt-3" data-testid="memory-empty">
              {t("memory.empty", undefined, "Nothing yet. Notes you accept or import appear here.")}
            </div>
          )
        ) : (
          <div className="mt-3 divide-y divide-line">
            {entries.map((m) => (
              <MemoryRow key={m.id} entry={m} onChanged={refresh} />
            ))}
          </div>
        )}
      </div>

      {/* Your instructions (§6): user-authored, toggle-independent — so it sits apart
          from the auto-memory pair above. The agent never edits these. */}
      <UserRulesCard settings={settings} onSaved={setSettings} />
    </section>
  );
}

function UserRulesCard({
  settings,
  onSaved,
}: {
  settings: MemorySettings;
  onSaved: (s: MemorySettings) => void;
}) {
  const { t } = useI18n();
  const [draft, setDraft] = useState(settings.user_rules);
  const [hasSavedMsg, setSavedMsg] = useState(false);
  const [isSaving, setSaving] = useState(false);
  const [hasError, setError] = useState(false);
  const savePending = useRef(false);

  const save = async () => {
    if (savePending.current) return;
    savePending.current = true;
    setSaving(true);
    setError(false);
    try {
      const next = await setMemorySettings({ user_rules: draft });
      onSaved(next);
      setSavedMsg(true);
      window.setTimeout(() => setSavedMsg(false), 3000);
    } catch {
      setSavedMsg(false);
      setError(true);
    } finally {
      savePending.current = false;
      setSaving(false);
    }
  };

  return (
    <div className={CARD + " p-4"} data-testid="user-rules-card">
      <div className={FIELD_LABEL}>{t("memory.rulesTitle", undefined, "Your instructions")}</div>
      <div className={FIELD_HELP}>
        {t("memory.rulesHelp", undefined, "Stored on this computer. Delta doesn't apply these to conversations yet.")}
      </div>
      <textarea
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        rows={4}
        placeholder={t("memory.rulesPlaceholder", undefined, "I use a screen reader — no tables, describe any image\nUse DD-MM-YYYY for dates")}
        data-testid="user-rules-input"
        aria-label={t("memory.rulesTitle", undefined, "Your instructions")}
        className="ds-field w-full mt-2.5 text-[13px] resize-y leading-relaxed"
      />
      <div className="flex items-center gap-3 mt-2">
        <Button variant="primary" size="sm" onClick={() => void save()}
          disabled={isSaving || draft === settings.user_rules} aria-busy={isSaving} data-testid="user-rules-save">
          {t("common.save", undefined, "Save")}
        </Button>
        {hasSavedMsg && (
          <span role="status" className="text-[12.5px] text-muted">
            {t("memory.rulesSaved", undefined, "Saved on this computer. Not applied to conversations yet.")}
          </span>
        )}
        {hasError && <span role="alert" className="text-[12.5px] text-danger">
          {t("memory.rulesFailed", undefined, "Couldn't save your instructions. Please try again.")}
        </span>}
      </div>
    </div>
  );
}

function CandidateRow({ candidate, onChanged }: { candidate: MemoryCandidate; onChanged: () => void }) {
  const { t } = useI18n();
  const [isEditing, setEditing] = useState(false);
  const [draft, setDraft] = useState(candidate.content);
  const [isBusy, setBusy] = useState(false);
  const [hasError, setError] = useState(false);
  const pending = useRef(false);
  const quote = candidate.source?.quote;

  const decide = async (action: () => Promise<{ ok: boolean }>) => {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setError(false);
    try {
      const result = await action();
      if (!result.ok) throw new Error("memory decision failed");
      onChanged();
    } catch {
      setError(true);
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };
  const accept = async (content?: string) =>
    decide(() => acceptMemoryCandidate(candidate.id, content));
  const decline = async () =>
    decide(() => rejectMemoryCandidate(candidate.id));

  return (
    <div className="py-2.5" data-testid={`memory-candidate-${candidate.id}`}>
      {isEditing ? (
        <textarea
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          rows={2}
          autoFocus
          disabled={isBusy}
          data-testid={`memory-candidate-input-${candidate.id}`}
          aria-label={t("memorySuggestion.edit", undefined, "Edit")}
          className="ds-field w-full text-[13px] resize-y leading-relaxed"
        />
      ) : (
        <div className="text-[13px] leading-relaxed">{candidate.content}</div>
      )}
      {quote && !isEditing && (
        <div className="text-[12px] text-faint mt-0.5">
          {t("memory.suggestionBecause", { quote }, `You said: “${quote}”`)}
        </div>
      )}
      {hasError && <div role="alert" className="text-[12px] text-danger mt-1.5">
        {t("memory.actionFailed", undefined, "Couldn't update that memory. Please try again.")}
      </div>}
      <div className="flex items-center gap-2.5 mt-1.5">
        {isEditing ? (
          <>
            <Button variant="primary" size="sm" disabled={isBusy || !draft.trim()} aria-busy={isBusy}
              data-testid={`memory-candidate-save-${candidate.id}`}
              onClick={() => void accept(draft.trim())}>
              {t("common.save", undefined, "Save")}
            </Button>
            <Button variant="ghost" size="sm" disabled={isBusy} onClick={() => setEditing(false)}>
              {t("common.cancel", undefined, "cancel")}
            </Button>
          </>
        ) : (
          <>
            <Button variant="primary" size="sm" disabled={isBusy} aria-busy={isBusy}
              data-testid={`memory-candidate-accept-${candidate.id}`} onClick={() => void accept()}>
              {t("memorySuggestion.accept", undefined, "Remember")}
            </Button>
            <Button variant="ghost" size="sm" disabled={isBusy}
              data-testid={`memory-candidate-edit-${candidate.id}`} onClick={() => {
                setDraft(candidate.content);
                setEditing(true);
              }}>
              {t("memorySuggestion.edit", undefined, "Edit")}
            </Button>
            <Button variant="ghost" size="sm" disabled={isBusy}
              data-testid={`memory-candidate-decline-${candidate.id}`} onClick={() => void decline()}>
              {t("memorySuggestion.decline", undefined, "No thanks")}
            </Button>
          </>
        )}
      </div>
    </div>
  );
}

function MemoryRow({ entry, onChanged }: { entry: MemoryEntry; onChanged: () => void }) {
  const { t } = useI18n();
  const [isEditing, setEditing] = useState(false);
  const [draft, setDraft] = useState(entry.content);
  const [isBusy, setBusy] = useState(false);
  const [hasError, setError] = useState(false);
  const pending = useRef(false);

  const runAction = async (action: () => Promise<{ ok: boolean }>, onSuccess: () => void) => {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setError(false);
    try {
      const result = await action();
      if (!result.ok) throw new Error("memory mutation failed");
      onSuccess();
      onChanged();
    } catch {
      setError(true);
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };
  const save = async () => {
    const value = draft.trim();
    if (!value || pending.current) return;
    if (value === entry.content) {
      setEditing(false);
      return;
    }
    await runAction(() => updateMemory(entry.id, value), () => setEditing(false));
  };
  const remove = async () =>
    runAction(() => deleteMemory(entry.id), () => {});

  if (isEditing)
    return (
      <div className="py-2.5" data-testid={`memory-edit-${entry.id}`}>
        <textarea
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          rows={2}
          autoFocus
          disabled={isBusy}
          aria-label={t("memory.fixTitle", undefined, "Fix this")}
          className="ds-field w-full text-[13px] resize-y leading-relaxed"
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              void save();
            }
            if (e.key === "Escape" && !isBusy) setEditing(false);
          }}
        />
        {hasError && <div role="alert" className="text-[12px] text-danger mt-1.5">
          {t("memory.actionFailed", undefined, "Couldn't update that memory. Please try again.")}
        </div>}
        <div className="flex items-center gap-2.5 mt-1.5">
          <Button variant="primary" size="sm" disabled={isBusy || !draft.trim()}
            aria-busy={isBusy} data-testid={`memory-save-${entry.id}`}
            onClick={() => void save()}>
            {t("common.save", undefined, "Save")}
          </Button>
          <Button variant="ghost" size="sm" disabled={isBusy} onClick={() => setEditing(false)}>
            {t("common.cancel", undefined, "cancel")}
          </Button>
        </div>
      </div>
    );

  return (
    <div className="py-2.5 flex items-start gap-2.5 group" data-testid={`memory-row-${entry.id}`}>
      <div className="min-w-0 flex-1 text-[13px] leading-relaxed">
        {entry.content}
        {hasError && <div role="alert" className="text-[12px] text-danger mt-1.5">
          {t("memory.actionFailed", undefined, "Couldn't update that memory. Please try again.")}
        </div>}
        {entry.saved_by === "candidate" && (
          <div className="text-[12px] text-faint mt-0.5" data-testid={`memory-origin-${entry.id}`}>
            {entry.source?.quote
              ? t("memory.suggestionBecause", { quote: entry.source.quote }, `You said: “${entry.source.quote}”`)
              : t("memory.suggestedAccepted", undefined, "Suggested by Delta, accepted by you")}
          </div>
        )}
      </div>
      <IconButton icon="pencil" iconSize={14} className="text-faint hover:text-ink shrink-0 mt-0.5"
        label={t("memory.fixTitle", undefined, "Fix this")}
        disabled={isBusy}
        data-testid={`memory-edit-btn-${entry.id}`}
        onClick={() => {
          setError(false);
          setDraft(entry.content);
          setEditing(true);
        }} />
      <IconButton icon="trash" iconSize={14} className="text-faint hover:text-danger shrink-0 mt-0.5"
        label={t("memory.deleteTitle", undefined, "Delete this memory")}
        disabled={isBusy} aria-busy={isBusy}
        data-testid={`memory-delete-${entry.id}`}
        onClick={() => void remove()} />
    </div>
  );
}
