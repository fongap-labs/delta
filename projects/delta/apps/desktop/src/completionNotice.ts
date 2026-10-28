// The notification shown when a task finishes while the window is in the background: the first
// lines of the answer, the files it made, and a click that opens the first file. The runtime
// decides what was delivered; this only words it.

export interface CompletionFile {
  path: string;
  name: string;
}

export interface CompletionNotice {
  title: string;
  body: string;
  /** The file a click opens, when the task made one. */
  openPath?: string;
}

type Translate = (key: string, vars?: Record<string, string | number>) => string;

const SUMMARY_CHARS = 120;
const LISTED_FILES = 3;

/** Plain text from a Markdown answer: links lose their address, markers go, whitespace collapses. */
export function plainSummary(markdown: string, limit = SUMMARY_CHARS): string {
  const plain = markdown
    .replace(/```[\s\S]*?```/g, " ")
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/^\s{0,3}#{1,6}\s+/gm, "")
    .replace(/^\s*(?:[-*+]|\d+[.)])\s+/gm, "")
    .replace(/[*_`>~]+/g, "")
    .replace(/\s+/g, " ")
    .trim();
  const chars = Array.from(plain);
  return chars.length <= limit ? plain : chars.slice(0, limit).join("").trimEnd() + "…";
}

export function buildCompletionNotice(input: {
  answer: string;
  files: CompletionFile[];
  t: Translate;
}): CompletionNotice {
  const { answer, files, t } = input;
  const parts = [plainSummary(answer) || t("notify.taskDone")];
  if (files.length > 0) {
    const names = files.slice(0, LISTED_FILES).map((file) => file.name).join(", ");
    parts.push(
      files.length > LISTED_FILES
        ? t("notify.filesMore", { names, more: files.length - LISTED_FILES })
        : t("notify.files", { names }),
    );
  }
  return {
    title: "Delta",
    body: parts.join("\n"),
    ...(files.length > 0 ? { openPath: files[0].path } : {}),
  };
}

/** Show the notice. A click brings the window forward and asks the app to open the file. */
export function showCompletionNotice(
  notice: CompletionNotice,
  onOpen: (path: string) => void,
  notification: typeof Notification | undefined = typeof Notification === "undefined" ? undefined : Notification,
  focusWindow: () => void = () => window.focus(),
): void {
  if (!notification) return;
  const show = () => {
    const shown = new notification(notice.title, { body: notice.body });
    shown.onclick = () => {
      focusWindow();
      if (notice.openPath) onOpen(notice.openPath);
    };
  };
  if (notification.permission === "granted") show();
  else if (notification.permission === "default") void notification.requestPermission().then((p) => p === "granted" && show());
}
