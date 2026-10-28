import { useCallback, useEffect, useState } from "react";
import {
  getLibraryRoots,
  indexLibraryRoot,
  pickWorkspaceFolder,
  removeLibraryRoot,
  type LibraryIndexReport,
  type LibraryRoot,
} from "../api";
import { useI18n } from "@delta/i18n/I18nContext";

const CARD = "rounded-xl2 border border-line bg-panel p-4 mb-4";
const BTN = "text-[12.5px] px-3 py-1.5 rounded-lg border border-line bg-bg hover:border-line-strong shrink-0 disabled:opacity-40";

/** The local library: the folders the user chose to make searchable. Indexing reads files on this
 *  computer only and sends nothing anywhere; a session searches only the folders it may read. */
export function LibrarySection() {
  const { t } = useI18n();
  const [roots, setRoots] = useState<LibraryRoot[] | null>(null);
  const [busyRoot, setBusyRoot] = useState<string | null>(null);
  const [message, setMessage] = useState("");

  const load = useCallback(() => {
    getLibraryRoots()
      .then(setRoots)
      .catch(() => setMessage(t("library.error.load")));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(load, [load]);

  const describe = (report: LibraryIndexReport) =>
    t(report.is_complete ? "library.done" : "library.partial", {
      indexed: report.files_indexed,
      unchanged: report.files_unchanged,
      removed: report.files_removed,
      skipped: report.files_skipped,
    });

  const index = async (root: string) => {
    setMessage("");
    setBusyRoot(root);
    try {
      setMessage(describe(await indexLibraryRoot(root)));
    } catch (error) {
      setMessage(t("library.error.index", { error: error instanceof Error ? error.message : String(error) }));
    } finally {
      setBusyRoot(null);
      load();
    }
  };

  const add = async () => {
    const folder = await pickWorkspaceFolder().catch(() => null);
    if (folder) await index(folder);
  };

  const remove = async (root: string) => {
    setMessage("");
    try {
      await removeLibraryRoot(root);
    } catch (error) {
      setMessage(t("library.error.remove", { error: error instanceof Error ? error.message : String(error) }));
    }
    load();
  };

  return (
    <div data-testid="library-section">
      <section className={CARD}>
        <div className="text-[13px] text-muted mb-3">{t("library.help")}</div>
        <div className="flex items-center gap-2 flex-wrap">
          <button className={BTN} disabled={busyRoot !== null} data-testid="library-add" onClick={() => void add()}>
            {t("library.add")}
          </button>
          <span className="text-[12px] text-muted" role="status" data-testid="library-message">
            {busyRoot ? t("library.indexing", { root: busyRoot }) : message}
          </span>
        </div>
      </section>
      <section className={CARD}>
        {roots !== null && roots.length === 0 && (
          <div className="text-[12.5px] text-muted" data-testid="library-empty">
            {t("library.empty")}
          </div>
        )}
        {(roots ?? []).map((root) => (
          <div key={root.root} className="flex items-start gap-3 py-2" data-testid="library-root">
            <div className="flex-1 min-w-0">
              <div className="text-[13px] font-medium break-all">{root.root}</div>
              <div className="text-[12px] text-muted">
                {t("library.stats", { files: root.files, chunks: root.chunks, version: root.version })}
                {root.skipped > 0 ? ` · ${t("library.skipped", { n: root.skipped })}` : ""}
                {root.indexed_at ? ` · ${root.indexed_at.slice(0, 16).replace("T", " ")}` : ""}
              </div>
            </div>
            <button className={BTN} disabled={busyRoot !== null} data-testid="library-update" onClick={() => void index(root.root)}>
              {t("library.update")}
            </button>
            <button className={BTN} disabled={busyRoot !== null} data-testid="library-remove" onClick={() => void remove(root.root)}>
              {t("library.remove")}
            </button>
          </div>
        ))}
      </section>
    </div>
  );
}
