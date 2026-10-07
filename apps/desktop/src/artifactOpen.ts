// Which files "Open" may hand to the operating system's default application.
//
// An artifact's name comes from the model and from files it wrote. Opening is a launch, so a
// crafted `report.pdf.exe`, a shortcut, a script or a file with no extension must never run: those
// are only revealed in the file manager. The list is deliberately small and positive (documents,
// plain text and common images). SVG and HTML are left out because they can carry script.
export const OPENABLE_EXTENSIONS: ReadonlySet<string> = new Set([
  "pdf",
  "docx",
  "xlsx",
  "pptx",
  "txt",
  "md",
  "csv",
  "png",
  "jpg",
  "jpeg",
  "gif",
  "webp",
]);

/** True when `path` ends in an extension that is safe to open with the default application. */
export function canOpenWithDefaultApp(path: string): boolean {
  const name = path.replace(/\\/g, "/").split("/").pop() ?? "";
  const dot = name.lastIndexOf(".");
  // No extension, a dot-file such as `.bashrc`, or a trailing dot (Windows would drop it).
  if (dot <= 0 || dot === name.length - 1) return false;
  return OPENABLE_EXTENSIONS.has(name.slice(dot + 1).toLowerCase());
}
