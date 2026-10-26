import { useEffect, useRef, useState } from "react";
import {
  deleteAllMemory,
  deleteMemory,
  exportMemory,
  getMemory,
  getMemorySettings,
  importMemory,
  setMemorySettings,
  updateMemory,
  MEMORY_CHANGED,
  type MemoryEntry,
  type MemorySettings,
} from "../api";
import { Icon } from "./Icon";
import { PanelHead } from "./IntegrationsView";
import { LoadError, LoadingRows } from "./ViewStates";
import { Toggle } from "./Toggle";
import { useI18n } from "@delta/i18n/I18nContext";

// MEMORY-SPEC §5.3: the one memory screen. A plain-language list of remembered facts
// (edit/delete per row), the on/off toggle, delete-all, and the User Rules textarea —
// no scope vocabulary, no markdown, no files. Everything else memory does happens in
// chat (toast §5.1, attribution §5.2).
const CARD = "rounded-xl2 border border-line bg-panel";
const FIELD_LABEL = "text-[12.5px] font-medium text-ink";
const FIELD_HELP = "text-[12px] text-muted mt-1.5 leading-relaxed";
const LINK_BTN = "text-[12px] text-muted hover:text-ink";
const BTN_ACCENT =
  "text-[12.5px] px-3 py-2 rounded-lg bg-primary text-on-primary shrink-0 disabled:opacity-40";

export function MemorySection() {
  const { t } = useI18n();
  const [settings, setSettings] = useState<MemorySettings | null>(null);
  const [entries, setEntries] = useState<MemoryEntry[] | null>(null);
  // State-change copy (§5.3): shown under the toggle / list after an action.
  const [toggleMsg, setToggleMsg] = useState<string | null>(null);
  const [listMsg, setListMsg] = useState<string | null>(null);
  // A failed read is not an empty memory: it shows an error with a retry instead of "Nothing yet".
  const [hasFailed, setFailed] = useState(false);
  const importInput = useRef<HTMLInputElement>(null);

  const refresh = () => {
    setFailed(false);
    getMemorySettings().then(setSettings).catch(() => setFailed(true));
    getMemory().then(setEntries).catch(() => setFailed(true));
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
        ? t("memory.enabledMsg", undefined, "Details you share from future conversations will be remembered, so I can be more helpful over time.")
        : t("memory.disabledMsg", undefined, "I'll stop remembering new things about you. What I already know is kept and still used — delete anything below you'd rather I forget."),
    );
  };

  const wipeAll = async () => {
    if (
      !window.confirm(
        t("memory.confirmDeleteAll", undefined, "Delete everything that's been remembered about you?\n\nThis can't be undone. Conversations you already have open still know what they knew — new conversations start with a clean slate."),
      )
    )
      return;
    await deleteAllMemory();
    setListMsg(
      t("memory.deletedMsg", undefined, "Everything I remembered has been deleted. New conversations start fresh; ones you already have open still know what they knew when they started."),
    );
    refresh();
  };

  // Export hands the user a `delta.memory` file; import reads one back. The runtime checks the
  // whole file and adds nothing when any entry is bad, so a wrong file cannot half-apply.
  const exportAll = async () => {
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
      setListMsg(t("memory.exportFailed", { error: reason }, `Couldn't export your memories: ${reason}`));
    }
  };

  const importFile = async (file: File) => {
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
        sub={t("memory.sub", undefined, "Memory isn't switched on yet. Delta doesn't save anything from your conversations, and nothing on this page is sent to a model. You can already keep, edit and delete notes here.")}
      />

      {/* On/off — one switch, no other setup (§5.4). */}
      <div className={CARD + " p-4 mb-4"} data-testid="memory-toggle-card">
        <div className="flex items-center gap-3">
          <Toggle checked={false} disabled onChange={toggleEnabled} title={t("memory.toggleTitle", undefined, "Remember new things about you")} />
          <div className="min-w-0 flex-1">
            <div className={FIELD_LABEL}>{t("memory.toggleLabel", undefined, "Remember new things about me")}</div>
            <div className="text-[12px] text-muted mt-0.5">
              {t("memory.toggleHelp", undefined, "Not available yet. Saving things from chat hasn't been built, so this switch is off and does nothing today.")}
            </div>
          </div>
        </div>
        {toggleMsg && (
          <div className="text-[12.5px] text-muted mt-3 pt-3 border-t border-line" data-testid="memory-toggle-msg">
            {toggleMsg}
          </div>
        )}
      </div>

      {/* What I've learned (§5.3): directly under the toggle that governs it — the off
          message ("what I already know is kept, delete it below") points here. */}
      <div className={CARD + " p-4 mb-4"} data-testid="memory-list-card">
        <div className="flex items-center gap-2">
          <div className={FIELD_LABEL + " flex-1"}>{t("memory.listTitle", undefined, "Notes about you")}</div>
          {entries.length > 0 && (
            <button className={LINK_BTN} data-testid="memory-export" onClick={() => void exportAll()}>
              {t("memory.export", undefined, "Export")}
            </button>
          )}
          <button className={LINK_BTN} data-testid="memory-import" onClick={() => importInput.current?.click()}>
            {t("memory.import", undefined, "Import")}
          </button>
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
            <button
              className="text-[12px] text-danger/80 hover:text-danger"
              data-testid="memory-delete-all"
              onClick={wipeAll}
            >
              {t("memory.forgetAll", undefined, "Forget everything…")}
            </button>
          )}
        </div>
        <div className={FIELD_HELP}>
          {t("memory.listHelp", undefined, "These notes are only stored on this computer for now, and Delta doesn't use them in conversations yet. Fix anything that's wrong — or delete it.")}
        </div>
        {listMsg && (
          <div className="text-[12.5px] text-muted mt-2.5" data-testid="memory-list-msg">
            {listMsg}
          </div>
        )}
        {entries.length === 0 ? (
          !listMsg && (
            <div className="text-[12px] text-muted mt-3" data-testid="memory-empty">
              {t("memory.empty", undefined, "Nothing yet. Notes you import appear here; nothing is saved from chat yet.")}
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

  const save = async () => {
    const next = await setMemorySettings({ user_rules: draft });
    onSaved(next);
    setSavedMsg(true);
    window.setTimeout(() => setSavedMsg(false), 3000);
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
        className="w-full mt-2.5 px-3 py-2.5 rounded-lg border border-line bg-bg text-[13px] text-ink outline-none focus:border-primary resize-y leading-relaxed"
      />
      <div className="flex items-center gap-3 mt-2">
        <button
          className={BTN_ACCENT}
          onClick={save}
          disabled={draft === settings.user_rules}
          data-testid="user-rules-save"
        >
          {t("common.save", undefined, "Save")}
        </button>
        {hasSavedMsg && (
          <span className="text-[12.5px] text-muted">
            {t("memory.rulesSaved", undefined, "Saved on this computer. Not applied to conversations yet.")}
          </span>
        )}
      </div>
    </div>
  );
}

function MemoryRow({ entry, onChanged }: { entry: MemoryEntry; onChanged: () => void }) {
  const { t } = useI18n();
  const [isEditing, setEditing] = useState(false);
  const [draft, setDraft] = useState(entry.content);

  const save = async () => {
    const text = draft.trim();
    if (text && text !== entry.content) await updateMemory(entry.id, text);
    setEditing(false);
    onChanged();
  };
  const remove = async () => {
    await deleteMemory(entry.id);
    onChanged();
  };

  if (isEditing)
    return (
      <div className="py-2.5" data-testid={`memory-edit-${entry.id}`}>
        <textarea
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          rows={2}
          autoFocus
          className="w-full px-3 py-2 rounded-lg border border-line bg-bg text-[13px] text-ink outline-none focus:border-primary resize-y leading-relaxed"
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              void save();
            }
            if (e.key === "Escape") setEditing(false);
          }}
        />
        <div className="flex items-center gap-2.5 mt-1.5">
          <button className={BTN_ACCENT} onClick={() => void save()}>
            {t("common.save", undefined, "Save")}
          </button>
          <button className="text-[12.5px] text-muted hover:text-ink" onClick={() => setEditing(false)}>
            {t("common.cancel", undefined, "cancel")}
          </button>
        </div>
      </div>
    );

  return (
    <div className="py-2.5 flex items-start gap-2.5 group" data-testid={`memory-row-${entry.id}`}>
      <div className="min-w-0 flex-1 text-[13px] leading-relaxed">{entry.content}</div>
      <button
        className="text-faint hover:text-ink shrink-0 mt-0.5"
        title={t("memory.fixTitle", undefined, "Fix this")}
        data-testid={`memory-edit-btn-${entry.id}`}
        onClick={() => {
          setDraft(entry.content);
          setEditing(true);
        }}
      >
        <Icon name="pencil" size={14} />
      </button>
      <button
        className="text-faint hover:text-danger shrink-0 mt-0.5"
        title={t("memory.deleteTitle", undefined, "Delete this memory")}
        data-testid={`memory-delete-${entry.id}`}
        onClick={() => void remove()}
      >
        <Icon name="trash" size={14} />
      </button>
    </div>
  );
}
