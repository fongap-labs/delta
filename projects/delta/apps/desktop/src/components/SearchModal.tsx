import { useEffect, useRef, useState } from "react";
import { searchConversations, searchLibrary, type ConversationHit, type LibraryHit } from "../api";
import type { SessionInfo } from "../types";
import { Icon } from "./Icon";
import { DialogSurface } from "./ui/DialogSurface";
import { FieldInput } from "./ui/FieldInput";
import { SegmentedControl } from "./ui/SegmentedControl";
import { baseName } from "../paths";
import { useI18n } from "@delta/i18n/I18nContext";

// Command-palette search (Codex-style): clicking Search opens this overlay over the whole app
// rather than filtering the sidebar in place (which made the grouped list collapse). It searches
// ALL sessions by title, split into Pinned + Recent, filters as you type, and supports ↑/↓ + Enter + ⌘1–9.
// Below the title matches it lists messages that contain the query, from the conversations of
// `workspace` only (ADR-0075), one row per session.
//
// A source switcher (Chats / Files) lets the user pick what to search: conversation history, or
// the local file library. Web search is a model tool that needs approval, so it stays out of the
// instant palette — the user asks the model for that.

const byRecent = (a: SessionInfo, b: SessionInfo) =>
  (b.updated_at || "").localeCompare(a.updated_at || "");

const CONTENT_MIN_CHARS = 2;
const CONTENT_DEBOUNCE_MS = 250;

type SearchSource = "chats" | "files";

export function SearchModal({
  sessions,
  workspace,
  onSelect,
  onClose,
}: {
  sessions: SessionInfo[];
  workspace?: string;
  onSelect: (id: string, workspace: string) => void;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const [source, setSource] = useState<SearchSource>("chats");
  const [hits, setHits] = useState<ConversationHit[]>([]);
  const [fileHits, setFileHits] = useState<LibraryHit[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  // Conversation search runs after the user pauses typing; a result that arrives after the query has
  // changed is dropped, so the list never shows hits for an older query.
  useEffect(() => {
    const trimmed = query.trim();
    if (source !== "chats" || !workspace || trimmed.length < CONTENT_MIN_CHARS) {
      setHits([]);
      return;
    }
    let isStale = false;
    const timer = window.setTimeout(() => {
      searchConversations(workspace, trimmed)
        .then((found) => {
          if (!isStale) setHits(found);
        })
        .catch(() => {
          if (!isStale) setHits([]);
        });
    }, CONTENT_DEBOUNCE_MS);
    return () => {
      isStale = true;
      window.clearTimeout(timer);
    };
  }, [workspace, query, source]);

  // Library search: same debounce, same staleness guard, different backend.
  useEffect(() => {
    const trimmed = query.trim();
    if (source !== "files" || !workspace || trimmed.length < CONTENT_MIN_CHARS) {
      setFileHits([]);
      return;
    }
    let isStale = false;
    const timer = window.setTimeout(() => {
      searchLibrary(workspace, trimmed)
        .then((found) => {
          if (!isStale) setFileHits(found);
        })
        .catch(() => {
          if (!isStale) setFileHits([]);
        });
    }, CONTENT_DEBOUNCE_MS);
    return () => {
      isStale = true;
      window.clearTimeout(timer);
    };
  }, [workspace, query, source]);

  // Escape must work regardless of where focus sits: the container-level onKeyDown only
  // fires while focus stays inside this subtree, so one click on a non-focusable spot of
  // the modal (header padding, list background) drops focus to <body> and Esc went dead.
  useEffect(() => {
    const onWinKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", onWinKey);
    return () => window.removeEventListener("keydown", onWinKey);
  }, [onClose]);

  const tagFor = (s: SessionInfo) => s.workspace ? baseName(s.workspace) : "Delta";

  const q = query.trim().toLowerCase();
  const real = sessions.filter((s) => !s.session_id.startsWith("__") && !s.archived);
  const match = (s: SessionInfo) =>
    !q ||
    (s.title || t("nav.untitledTask", undefined, "New task")).toLowerCase().includes(q) ||
    tagFor(s).toLowerCase().includes(q);

  const filtered = real.filter(match);
  const pinned = filtered.filter((s) => s.pinned).sort(byRecent);
  const recent = filtered.filter((s) => !s.pinned).sort(byRecent);
  const ordered = [...pinned, ...recent]; // flat order drives keyboard nav + ⌘N

  // One message row per session, and only for sessions whose title did not already match.
  const titled = new Set(filtered.map((s) => s.session_id));
  const contentHits = hits.filter(
    (h, i) => !titled.has(h.session_id) && hits.findIndex((x) => x.session_id === h.session_id) === i,
  );
  // Keyboard positions: session rows first, then message rows.
  const total = source === "chats" ? ordered.length + contentHits.length : fileHits.length;

  // Reset the highlight whenever the result set or source changes.
  useEffect(() => {
    setActive(0);
  }, [q, source]);

  const choose = (s?: SessionInfo) => {
    const target = s || ordered[active];
    if (!target) return;
    onSelect(target.session_id, target.workspace);
    onClose();
  };

  const chooseHit = (h: ConversationHit) => {
    if (!workspace) return;
    onSelect(h.session_id, workspace);
    onClose();
  };

  const onKey = (e: React.KeyboardEvent) => {
    // Source-switch keyboard events belong to SegmentedControl, not result navigation.
    if ((e.target as HTMLElement).closest(".ds-search-source-switch")) return;
    if (e.key === "Escape") {
      return; // handled by the window-level listener above (focus-independent)
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((i) => Math.min(i + 1, total - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (source === "files") {
        return;
      }
      if (active < ordered.length || !contentHits[active - ordered.length]) {
        choose();
      } else {
        chooseHit(contentHits[active - ordered.length]);
      }
    } else if ((e.metaKey || e.ctrlKey) && e.key >= "1" && e.key <= "9") {
      const idx = Number(e.key) - 1;
      if (source === "chats" && ordered[idx]) {
        e.preventDefault();
        choose(ordered[idx]);
      }
    }
  };

  const row = (s: SessionInfo, idx: number) => {
    const isActive = idx === active;
    return (
      <button
        key={s.session_id}
        className={
          "w-full flex items-center gap-3 px-3 py-2 rounded-lg text-left " +
          (isActive ? "bg-selected" : "hover:bg-bg")
        }
        onMouseEnter={() => setActive(idx)}
        onClick={() => choose(s)}
      >
        <span className="min-w-0 flex-1 truncate text-[13.5px] text-ink">
          {s.title || t("nav.untitledTask", undefined, "New task")}
        </span>
        <span className="text-[12px] text-faint shrink-0">{tagFor(s)}</span>
        {idx < 9 && (
          <kbd className="text-[10.5px] text-faint bg-bg border border-line rounded px-1.5 py-0.5 shrink-0 font-sans">
            ⌘{idx + 1}
          </kbd>
        )}
      </button>
    );
  };

  const hitRow = (h: ConversationHit, idx: number) => {
    const isActive = idx === active;
    return (
      <button
        key={`${h.session_id}:${h.line}`}
        className={
          "w-full flex flex-col gap-0.5 px-3 py-2 rounded-lg text-left " +
          (isActive ? "bg-selected" : "hover:bg-bg")
        }
        onMouseEnter={() => setActive(idx)}
        onClick={() => chooseHit(h)}
      >
        <span className="flex w-full items-center gap-3">
          <span className="min-w-0 flex-1 truncate text-[13.5px] text-ink">
            {h.title || t("nav.untitledTask", undefined, "New task")}
          </span>
          <span className="text-[11px] text-faint shrink-0">
            {t("search.line", { n: h.line }, `line ${h.line}`)}
          </span>
        </span>
        <span className="min-w-0 truncate text-[12.5px] text-muted">{h.snippet}</span>
      </button>
    );
  };

  const fileRow = (h: LibraryHit, idx: number) => {
    const isActive = idx === active;
    const fileName = h.path.split(/[\\/]/).pop() || h.path;
    return (
      <div
        key={`${h.path}:${idx}`}
        className={
          "w-full flex flex-col gap-0.5 px-3 py-2 rounded-lg text-left " +
          (isActive ? "bg-selected" : "hover:bg-bg")
        }
        onMouseEnter={() => setActive(idx)}
      >
        <span className="flex w-full items-center gap-3">
          <span className="min-w-0 flex-1 truncate text-[13.5px] text-ink">{fileName}</span>
          {h.stale && (
            <span className="text-[11px] text-faint shrink-0">stale</span>
          )}
        </span>
        <span className="min-w-0 truncate text-[12.5px] text-muted">{h.snippet}</span>
        <span className="min-w-0 truncate text-[11px] text-faint">{h.path}</span>
      </div>
    );
  };

  return (
    <div className="fixed inset-0 z-50" onKeyDown={onKey}>
      <div className="ds-dialog-backdrop absolute inset-0 backdrop-blur-[1px]" onClick={onClose} />
      <DialogSurface label={t("search.placeholder", undefined, "Search chats")} className="ds-search-palette ds-focus-surface absolute left-1/2 top-[14vh] -translate-x-1/2 w-[640px] max-w-[92vw] overflow-hidden">
        <div className="px-4 pt-3.5 pb-2.5 border-b border-line flex items-center gap-2.5">
          <Icon name="search" size={16} className="text-faint shrink-0" />
          <FieldInput
            appearance="embedded"
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={
              source === "chats"
                ? t("search.placeholder", undefined, "Search chats")
                : t("search.placeholderFiles", undefined, "Search your files")
            }
            aria-label={source === "chats" ? t("search.placeholder", undefined, "Search chats") : t("search.placeholderFiles", undefined, "Search your files")}
            className="flex-1 text-[15px]"
          />
          <SegmentedControl
            className="ds-search-source-switch shrink-0"
            label={t("search.sourceChats", undefined, "Chats") + " / " + t("search.sourceFiles", undefined, "Files")}
            value={source}
            options={[
              { value: "chats", label: t("search.sourceChats", undefined, "Chats"), testId: "search-source-chats" },
              { value: "files", label: t("search.sourceFiles", undefined, "Files"), testId: "search-source-files" },
            ]}
            onChange={setSource}
          />
          <kbd className="text-[10.5px] text-faint bg-bg border border-line rounded px-1.5 py-0.5 font-sans">
            Esc
          </kbd>
        </div>
        <div className="max-h-[52vh] overflow-y-auto hairline-scroll py-2">
          {source === "chats" ? (
            total === 0 ? (
              <div className="px-4 py-8 text-center text-[13px] text-faint">
                {t("search.noResults", undefined, "No chats found.")}
              </div>
            ) : (
              <>
                {pinned.length > 0 && (
                  <div className="px-2">
                    <div className="px-2 py-1 text-[11px] uppercase tracking-[0.05em] text-faint font-semibold">
                      {t("search.pinned", undefined, "Pinned chats")}
                    </div>
                    {pinned.map((s, i) => row(s, i))}
                  </div>
                )}
                {recent.length > 0 && (
                  <div className="px-2 mt-1">
                    <div className="px-2 py-1 text-[11px] uppercase tracking-[0.05em] text-faint font-semibold">
                      {t("search.recent", undefined, "Recent chats")}
                    </div>
                    {recent.map((s, i) => row(s, pinned.length + i))}
                  </div>
                )}
                {contentHits.length > 0 && (
                  <div className="px-2 mt-1">
                    <div className="px-2 py-1 text-[11px] uppercase tracking-[0.05em] text-faint font-semibold">
                      {t("search.inMessages", undefined, "In messages")}
                    </div>
                    {contentHits.map((h, i) => hitRow(h, ordered.length + i))}
                  </div>
                )}
              </>
            )
          ) : (
            fileHits.length === 0 ? (
              <div className="px-4 py-8 text-center text-[13px] text-faint">
                {q.length < CONTENT_MIN_CHARS
                  ? t("search.typeToSearch", undefined, "Type to search your files.")
                  : t("search.noFiles", undefined, "No files found.")}
              </div>
            ) : (
              <div className="px-2">
                <div className="px-2 py-1 text-[11px] uppercase tracking-[0.05em] text-faint font-semibold">
                  {t("search.inFiles", undefined, "In your files")}
                </div>
                {fileHits.map((h, i) => fileRow(h, i))}
              </div>
            )
          )}
        </div>
      </DialogSurface>
    </div>
  );
}
