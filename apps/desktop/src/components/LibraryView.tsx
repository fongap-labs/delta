import { useEffect, useState } from "react";
import {
  getLibrary,
  pickWorkspaceFolder,
  readLibrarySource,
  removeLibrarySource,
  setLibraryFolder,
  writeLibrarySource,
  type LibrarySource,
} from "../api";
import { PanelHead } from "./IntegrationsView";
import { LoadError, LoadingRows } from "./ViewStates";
import { useI18n } from "@delta/i18n/I18nContext";
import { Icon } from "./Icon";

// Shared utility strings (the §28 page shell — mirrors ScheduledView's constants).
const CARD = "rounded-xl2 border border-line bg-panel";
const FIELD_LABEL = "text-[12px] text-muted";

function Shell({ children }: { children: React.ReactNode }) {
  return (
    <main className="flex-1 min-w-0 flex bg-bg">
      <div className="flex-1 min-w-0 overflow-y-auto hairline-scroll">
        <div className="max-w-4xl mx-auto px-7 py-6">{children}</div>
      </div>
    </main>
  );
}

const formatSize = (bytes: number) =>
  bytes < 1024 ? `${bytes} B` : `${(bytes / 1024).toFixed(1)} KB`;

const formatModified = (seconds: number | null) =>
  seconds ? new Date(seconds * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" }) : "—";

export function LibraryView() {
  const { t } = useI18n();
  const [folder, setFolder] = useState("");
  const [sources, setSources] = useState<LibrarySource[]>([]);
  const [isLoading, setLoading] = useState(true);
  const [hasFailed, setFailed] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [isCreating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [openName, setOpenName] = useState<string | null>(null);
  const [editorContent, setEditorContent] = useState("");
  const [isSaving, setSaving] = useState(false);
  const [savedName, setSavedName] = useState<string | null>(null);
  const [confirmRemove, setConfirmRemove] = useState<string | null>(null);

  const refresh = () =>
    getLibrary()
      .then((info) => {
        setFolder(info.folder);
        setSources(info.sources);
        setFailed(false);
      })
      .catch(() => setFailed(true))
      .finally(() => setLoading(false));
  useEffect(() => {
    refresh();
  }, []);

  const openSource = (name: string) => {
    setActionError(null);
    readLibrarySource(name)
      .then((res) => {
        if (res.ok && typeof res.content === "string") {
          setOpenName(name);
          setEditorContent(res.content);
          setSavedName(null);
        } else {
          setActionError(res.error || name);
        }
      })
      .catch(() => setActionError(name));
  };

  const saveSource = async () => {
    if (!openName) return;
    setSaving(true);
    setActionError(null);
    try {
      const res = await writeLibrarySource(openName, editorContent);
      if (res.ok) {
        setSavedName(openName);
        await refresh();
      } else {
        setActionError(res.error || openName);
      }
    } finally {
      setSaving(false);
    }
  };

  const removeSource = async (name: string) => {
    setActionError(null);
    setConfirmRemove(null);
    const res = await removeLibrarySource(name).catch(() => null);
    if (!res || !res.ok) {
      setActionError(res?.error || name);
      return;
    }
    if (openName === name) setOpenName(null);
    await refresh();
  };

  const changeFolder = async () => {
    setActionError(null);
    const picked = await pickWorkspaceFolder();
    if (!picked) return;
    const res = await setLibraryFolder(picked).catch(() => null);
    if (!res || !res.ok) {
      setActionError(res?.error || picked);
      return;
    }
    setOpenName(null);
    await refresh();
  };

  const createSource = async () => {
    const name = newName.trim();
    if (!name) return;
    setActionError(null);
    const res = await writeLibrarySource(name, "").catch(() => null);
    if (!res || !res.ok) {
      setActionError(
        res?.error || t("library.invalidName", undefined, "That file name is not allowed; use .md, .markdown or .txt."),
      );
      return;
    }
    setNewName("");
    setCreating(false);
    await refresh();
    openSource(res.name || name);
  };

  return (
    <Shell>
      <div className="flex items-start gap-3">
        <div className="flex-1 min-w-0">
          <PanelHead title={t("library.title", undefined, "Library")} sub={t("library.empty", undefined, "Your corpus of sources.")} />
        </div>
        <button
          className="text-[12.5px] px-3 py-1.5 rounded-lg border border-line-strong bg-panel hover:border-primary hover:text-primary shrink-0"
          data-testid="library-new"
          onClick={() => setCreating((v) => !v)}
        >
          {t("library.newSource", undefined, "New source")}
        </button>
      </div>

      <div className={CARD + " mt-4 p-3 flex items-center gap-2 flex-wrap"}>
        <Icon name="folder" size={14} className="shrink-0 text-muted" />
        <span className={FIELD_LABEL}>{t("library.folderLabel", undefined, "Folder")}</span>
        <span className="flex-1 min-w-0 text-[12px] font-mono text-faint truncate" data-testid="library-folder" title={folder}>
          {folder || t("library.noFolder", undefined, "Choose a folder.")}
        </span>
        <button
          className="text-[12px] px-2.5 py-1 rounded-lg border border-line bg-panel hover:border-primary hover:text-primary shrink-0"
          data-testid="library-change-folder"
          onClick={changeFolder}
        >
          {t("library.changeFolder", undefined, "Change location")}
        </button>
      </div>

      {actionError && <p className="text-[11.5px] text-warn mt-2">{actionError}</p>}

      {isCreating && (
        <div className={CARD + " mt-3 p-3 flex items-center gap-2"}>
          <input
            className="flex-1 min-w-0 px-2.5 py-1.5 rounded-lg bg-bg border border-line text-[13px] text-ink outline-none focus:border-primary"
            placeholder={t("library.sourceNamePlaceholder", undefined, "e.g. notes.md")}
            aria-label={t("library.sourceNamePlaceholder", undefined, "e.g. notes.md")}
            data-testid="library-new-name"
            value={newName}
            autoFocus
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") createSource();
              if (e.key === "Escape") setCreating(false);
            }}
          />
          <button
            className="text-[12.5px] px-3 py-1.5 rounded-lg bg-primary text-on-primary shrink-0"
            onClick={createSource}
          >
            {t("common.create", undefined, "Create")}
          </button>
        </div>
      )}

      {openName && (
        <div className={CARD + " mt-3 p-3"} data-testid="library-editor">
          <div className="flex items-center gap-2 mb-2">
            <Icon name="file" size={14} className="shrink-0 text-muted" />
            <span className="text-[13px] text-ink font-medium flex-1 min-w-0 truncate">{openName}</span>
            {savedName === openName && <span className="text-[11.5px] text-ok">{t("library.saved", undefined, "Saved")}</span>}
            <button
              className="text-[12px] px-2.5 py-1 rounded-lg border border-line bg-panel hover:border-primary hover:text-primary"
              data-testid="library-save"
              disabled={isSaving}
              onClick={saveSource}
            >
              {t("library.save", undefined, "Save")}
            </button>
            <button
              className="text-[12px] px-2 py-1 rounded-lg text-muted hover:text-ink"
              onClick={() => setOpenName(null)}
            >
              <Icon name="x" size={14} />
            </button>
          </div>
          <textarea
            className="w-full h-72 px-3 py-2 rounded-lg bg-bg border border-line text-[13px] text-ink font-mono outline-none focus:border-primary resize-y"
            value={editorContent}
            onChange={(e) => {
              setEditorContent(e.target.value);
              setSavedName(null);
            }}
          />
        </div>
      )}

      {sources.length === 0 && !isLoading && !hasFailed && (
        <p className="text-[13px] text-faint mt-6">{t("library.empty", undefined, "Your corpus of sources.")}</p>
      )}

      {sources.length > 0 && (
        <div className={CARD + " mt-3 divide-y divide-line"} data-testid="library-sources">
          <div className="text-[11px] text-faint flex items-center gap-2 px-3 py-2">
            <span className="flex-1 min-w-0">{t("library.sourceName", undefined, "Name")}</span>
            <span className="w-24 text-right">{t("library.sourceSize", undefined, "Size")}</span>
            <span className="w-44 text-right">{t("library.sourceModified", undefined, "Modified")}</span>
            <span className="w-16" />
          </div>
          {sources.map((source) => (
            <div key={source.name} className="flex items-center gap-2 px-3 py-2 hover:bg-bg">
              <button
                className="flex-1 min-w-0 text-left text-[13px] text-ink truncate"
                title={source.name}
                onClick={() => openSource(source.name)}
              >
                {source.name}
              </button>
              <span className="w-24 text-right text-[12px] text-faint">{formatSize(source.size)}</span>
              <span className="w-44 text-right text-[12px] text-faint truncate">{formatModified(source.modified)}</span>
              <span className="w-16 flex justify-end">
                {confirmRemove === source.name ? (
                  <button
                    className="text-[11.5px] px-2 py-1 rounded-lg text-danger bg-danger-soft"
                    onClick={() => removeSource(source.name)}
                  >
                    {t("common.confirm", undefined, "Confirm")}
                  </button>
                ) : (
                  <button
                    className="text-faint hover:text-danger p-1"
                    aria-label={t("library.remove", undefined, "Remove")}
                    onClick={() => setConfirmRemove(source.name)}
                  >
                    <Icon name="trash" size={14} />
                  </button>
                )}
              </span>
            </div>
          ))}
        </div>
      )}

      {isLoading && <LoadingRows count={3} />}
      {hasFailed && (
        <div className="mt-6">
          <LoadError onRetry={refresh} what={t("library.title", undefined, "Library")} />
        </div>
      )}
    </Shell>
  );
}
