import { beforeEach, describe, expect, it, vi } from "vitest";

const openPath = vi.fn(async (_path: string) => {});
const revealItemInDir = vi.fn(async (_path: string) => {});
let resolved: { ok: boolean; path?: string; error?: string } = { ok: true, path: "C:\\work\\a.pdf" };

vi.mock("@tauri-apps/plugin-opener", () => ({
  openPath: (path: string) => openPath(path),
  revealItemInDir: (path: string) => revealItemInDir(path),
}));
vi.mock("./runtimeTransport", () => ({
  resolveArtifactPath: async () => resolved,
}));
vi.mock("pdfjs-dist/build/pdf.worker.min.mjs?url", () => ({ default: "pdf.worker.js" }));

import { canOpenWithDefaultApp, OPENABLE_EXTENSIONS } from "./artifactOpen";
import { revealArtifact } from "./api";

describe("canOpenWithDefaultApp", () => {
  it.each([
    "C:\\work\\report.pdf",
    "/home/u/notes.TXT",
    "a/b/Sheet.XLSX",
    "deck.pptx",
    "doc.docx",
    "data.csv",
    "readme.md",
    "p.png",
    "p.JPG",
    "p.jpeg",
    "p.gif",
    "p.webp",
  ])("allows %s", (path) => {
    expect(canOpenWithDefaultApp(path)).toBe(true);
  });

  it.each([
    "run.bat",
    "run.cmd",
    "tool.exe",
    "setup.msi",
    "link.lnk",
    "shortcut.url",
    "page.hta",
    "screen.scr",
    "app.jar",
    "script.vbs",
    "script.js",
    "script.ps1",
    "config.reg",
    "script.sh",
    "page.html",
    "image.svg",
    "report.pdf.exe",
    "report.pdf .exe",
    "report.exe.pdf.lnk",
    "noextension",
    "C:\\work\\README",
    ".bashrc",
    "trailing.dot.",
    "dir.pdf/",
    "file.pdf:stream",
    "file.pdf ",
    "",
  ])("refuses %j", (path) => {
    expect(canOpenWithDefaultApp(path)).toBe(false);
  });

  it("keeps the allow list small and lower case", () => {
    expect([...OPENABLE_EXTENSIONS].sort()).toEqual(
      ["csv", "docx", "gif", "jpeg", "jpg", "md", "pdf", "png", "pptx", "txt", "webp", "xlsx"],
    );
  });
});

describe("revealArtifact", () => {
  beforeEach(() => {
    openPath.mockClear();
    revealItemInDir.mockClear();
    resolved = { ok: true, path: "C:\\work\\a.pdf" };
  });

  it("opens a safe document with the default application", async () => {
    await expect(revealArtifact("s", "a.pdf", "open")).resolves.toEqual({ ok: true });
    expect(openPath).toHaveBeenCalledWith("C:\\work\\a.pdf");
    expect(revealItemInDir).not.toHaveBeenCalled();
  });

  it.each(["x.bat", "x.lnk", "x.exe", "README", "x.pdf.exe", "x.svg"])("only reveals %s when open is requested", async (name) => {
    resolved = { ok: true, path: `C:\\work\\${name}` };
    await expect(revealArtifact("s", name, "open")).resolves.toEqual({ ok: true, downgraded: true });
    expect(openPath).not.toHaveBeenCalled();
    expect(revealItemInDir).toHaveBeenCalledWith(`C:\\work\\${name}`);
  });

  it("reveal mode never opens anything", async () => {
    await expect(revealArtifact("s", "a.pdf")).resolves.toEqual({ ok: true });
    expect(openPath).not.toHaveBeenCalled();
    expect(revealItemInDir).toHaveBeenCalledOnce();
  });

  it("reports an unavailable artifact without touching the file manager", async () => {
    resolved = { ok: false, error: "outside the session" };
    await expect(revealArtifact("s", "../x.pdf", "open")).resolves.toEqual({ ok: false, error: "outside the session" });
    expect(openPath).not.toHaveBeenCalled();
    expect(revealItemInDir).not.toHaveBeenCalled();
  });
});
