// Library (资料库) smoke: folder row, source list and the editor open from a source click.
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { LibraryView } from "./LibraryView";
import { I18nProvider } from "@delta/i18n/I18nContext";
import type { LibrarySource } from "../api";

const { getLibraryMock, readMock, writeMock, removeMock, setFolderMock, pickMock } = vi.hoisted(() => ({
  getLibraryMock: vi.fn(),
  readMock: vi.fn(),
  writeMock: vi.fn(),
  removeMock: vi.fn(),
  setFolderMock: vi.fn(),
  pickMock: vi.fn(),
}));

vi.mock("../api", async (importOriginal) => {
  const original = await importOriginal<typeof import("../api")>();
  return {
    ...original,
    getLibrary: (...a: unknown[]) => getLibraryMock(...a),
    readLibrarySource: (...a: unknown[]) => readMock(...a),
    writeLibrarySource: (...a: unknown[]) => writeMock(...a),
    removeLibrarySource: (...a: unknown[]) => removeMock(...a),
    setLibraryFolder: (...a: unknown[]) => setFolderMock(...a),
    pickWorkspaceFolder: (...a: unknown[]) => pickMock(...a),
  };
});

const source = (over: Partial<LibrarySource>): LibrarySource => ({
  name: "notes.md",
  size: 2048,
  modified: 1761000000,
  ...over,
});

const wrap = () => (
  <I18nProvider locale="en-US">
    <LibraryView />
  </I18nProvider>
);

describe("Library view", () => {
  afterEach(cleanup);

  it("shows the folder and the sources with sizes", async () => {
    getLibraryMock.mockResolvedValue({
      folder: "/home/u/corpus",
      sources: [source(), source({ name: "design.md", size: 512 })],
    });
    render(wrap());
    expect(await screen.findByTestId("library-folder").then((el) => el.textContent)).toBe("/home/u/corpus");
    const rows = screen.getByTestId("library-sources");
    expect(rows.textContent).toContain("notes.md");
    expect(rows.textContent).toContain("design.md");
    expect(rows.textContent).toContain("2.0 KB");
  });

  it("opening a source loads its body into the editor and saving writes it back", async () => {
    getLibraryMock.mockResolvedValue({ folder: "/c", sources: [source()] });
    readMock.mockResolvedValue({ ok: true, name: "notes.md", content: "hello corpus" });
    writeMock.mockResolvedValue({ ok: true, name: "notes.md" });
    render(wrap());
    fireEvent.click(await screen.findByText("notes.md"));
    const editor = await screen.findByTestId("library-editor");
    const area = editor.querySelector("textarea") as HTMLTextAreaElement;
    expect(area.value).toBe("hello corpus");
    fireEvent.change(area, { target: { value: "updated" } });
    fireEvent.click(screen.getByTestId("library-save"));
    await waitFor(() => expect(writeMock).toHaveBeenCalledWith("notes.md", "updated"));
  });

  it("the folder picker applies the chosen location", async () => {
    getLibraryMock.mockResolvedValue({ folder: "/old", sources: [] });
    pickMock.mockResolvedValue("/new-corpus");
    setFolderMock.mockResolvedValue({ ok: true, folder: "/new-corpus" });
    getLibraryMock.mockResolvedValueOnce({ folder: "/old", sources: [] });
    getLibraryMock.mockResolvedValue({ folder: "/new-corpus", sources: [] });
    render(wrap());
    fireEvent.click(await screen.findByTestId("library-change-folder"));
    await waitFor(() => expect(setFolderMock).toHaveBeenCalledWith("/new-corpus"));
    await waitFor(() => expect(screen.getByTestId("library-folder").textContent).toBe("/new-corpus"));
  });

  it("a failed read is reported inline, not as a crash", async () => {
    getLibraryMock.mockResolvedValue({ folder: "/c", sources: [source()] });
    readMock.mockResolvedValue({ ok: false, error: "unreadable" });
    render(wrap());
    fireEvent.click(await screen.findByText("notes.md"));
    expect(await screen.findByText("unreadable")).toBeTruthy();
    expect(screen.queryByTestId("library-editor")).toBeNull();
  });
});
