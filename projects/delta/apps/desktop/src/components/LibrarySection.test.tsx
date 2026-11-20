import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@delta/i18n/I18nContext";
import { LibrarySection } from "./LibrarySection";
import {
  getLibraryRoots, indexLibraryRoot, pickWorkspaceFolder, removeLibraryRoot,
  type LibraryIndexReport, type LibraryRoot,
} from "../api";

vi.mock("../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../api")>()),
  getLibraryRoots: vi.fn(),
  indexLibraryRoot: vi.fn(),
  pickWorkspaceFolder: vi.fn(),
  removeLibraryRoot: vi.fn(),
}));

const ROOT: LibraryRoot = {
  root: "/work/docs", files: 5, chunks: 20, version: 2, skipped: 0, indexed_at: null,
};
const REPORT: LibraryIndexReport = {
  root: "/work/docs", version: 3, files_seen: 5, files_indexed: 2,
  files_unchanged: 3, files_skipped: 0, files_removed: 0,
  chunks: 20, is_complete: true, seconds: 0.2,
};
const view = () => render(<I18nProvider locale="en-US"><LibrarySection /></I18nProvider>);

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(getLibraryRoots).mockResolvedValue([ROOT]);
  vi.mocked(indexLibraryRoot).mockResolvedValue(REPORT);
  vi.mocked(pickWorkspaceFolder).mockResolvedValue("/work/docs");
  vi.mocked(removeLibraryRoot).mockResolvedValue(true);
});
afterEach(cleanup);

describe("local library shared controls", () => {
  it("uses a shared accessible action for each indexed folder", async () => {
    view();
    const row = await screen.findByTestId("library-root");
    expect(row.textContent).toContain("/work/docs");
    const update = screen.getByRole("button", { name: "Update: /work/docs" });
    const remove = screen.getByRole("button", { name: "Remove: /work/docs" });
    expect(update.className).toContain("ds-button-sm");
    expect(remove.className).toContain("ds-button-danger");
    expect(update.getAttribute("type")).toBe("button");
    fireEvent.click(update);
    await waitFor(() => expect(indexLibraryRoot).toHaveBeenCalledWith("/work/docs"));
    expect(removeLibraryRoot).not.toHaveBeenCalled();
  });

  it("indexes only after an explicit folder selection and disables other actions while indexing", async () => {
    let complete!: (report: LibraryIndexReport) => void;
    vi.mocked(indexLibraryRoot).mockImplementation(() => new Promise((resolve) => { complete = resolve; }));
    view();
    await screen.findByTestId("library-root");
    expect(pickWorkspaceFolder).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTestId("library-add"));
    await waitFor(() => expect(indexLibraryRoot).toHaveBeenCalledWith("/work/docs"));
    const add = screen.getByTestId("library-add") as HTMLButtonElement;
    expect(add.disabled).toBe(true);
    expect(add.getAttribute("aria-busy")).toBe("true");
    expect((screen.getByTestId("library-remove") as HTMLButtonElement).disabled).toBe(true);
    await act(async () => { complete(REPORT); });
    await waitFor(() => expect(add.disabled).toBe(false));
    expect(screen.getByRole("status").textContent).toContain("Done:");
  });

  it("removes only the selected library root following an explicit click", async () => {
    view();
    await screen.findByTestId("library-root");
    expect(removeLibraryRoot).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTestId("library-remove"));
    await waitFor(() => expect(removeLibraryRoot).toHaveBeenCalledWith("/work/docs"));
    expect(indexLibraryRoot).not.toHaveBeenCalled();
  });
});
