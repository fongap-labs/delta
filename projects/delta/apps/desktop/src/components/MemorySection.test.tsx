import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@delta/i18n/I18nContext";
import { MemorySection } from "./MemorySection";
import {
  acceptMemoryCandidate, deleteAllMemory, deleteMemory, exportMemory,
  getMemory, getMemoryCandidates, getMemorySettings, importMemory,
  rejectMemoryCandidate, setMemorySettings, updateMemory,
} from "../api";

vi.mock("../api", () => ({
  MEMORY_CHANGED: "delta:memory-changed",
  acceptMemoryCandidate: vi.fn(),
  deleteAllMemory: vi.fn(),
  deleteMemory: vi.fn(),
  exportMemory: vi.fn(),
  getMemory: vi.fn(),
  getMemoryCandidates: vi.fn(),
  getMemorySettings: vi.fn(),
  importMemory: vi.fn(),
  rejectMemoryCandidate: vi.fn(),
  setMemorySettings: vi.fn(),
  updateMemory: vi.fn(),
}));

const SETTINGS = { enabled: true, user_rules: "Use citations" };
const ENTRY = {
  id: 14, scope: "global", content: "Prefers metric units", summary: "Measurement",
  created_at: "2026-11-01T10:00:00Z", saved_by: "user",
};
const CANDIDATE = {
  id: 23, content: "Prefers Chinese reports", source: null,
  created_at: "2026-11-01T12:00:00Z",
};
const view = () => render(
  <I18nProvider locale="en-US"><MemorySection /></I18nProvider>,
);

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(getMemorySettings).mockResolvedValue(SETTINGS);
  vi.mocked(getMemory).mockResolvedValue([ENTRY]);
  vi.mocked(getMemoryCandidates).mockResolvedValue([CANDIDATE]);
  vi.mocked(setMemorySettings).mockImplementation(async (patch) => ({ ...SETTINGS, ...patch }));
  vi.mocked(deleteAllMemory).mockResolvedValue({ ok: true, deleted: 1 });
  vi.mocked(deleteMemory).mockResolvedValue({ ok: true });
  vi.mocked(updateMemory).mockResolvedValue({ ok: true, id: 15 });
  vi.mocked(acceptMemoryCandidate).mockResolvedValue({ ok: true, memory_id: 17 });
  vi.mocked(rejectMemoryCandidate).mockResolvedValue({ ok: true });
  vi.mocked(importMemory).mockResolvedValue({ ok: true, imported: 1, skipped: 0 });
  vi.mocked(exportMemory).mockResolvedValue({ format: "delta.memory", version: 1, memories: [] });
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

describe("memory management shared controls", () => {
  it("shows shared accessible controls and leaves memory decisions explicit", async () => {
    view();
    const list = await screen.findByTestId("memory-list-card");
    const accept = await screen.findByTestId("memory-candidate-accept-23");
    expect(accept.className).toContain("ds-button-primary");
    expect(screen.getByTestId("memory-candidate-decline-23").className).toContain("ds-button-ghost");
    expect(screen.getByTestId("memory-export").className).toContain("ds-button-ghost");
    expect(screen.getByTestId("memory-import").className).toContain("ds-button-ghost");
    expect(screen.getByTestId("memory-delete-all").className).toContain("ds-button-danger");
    expect(screen.getByTestId("memory-edit-btn-14").className).toContain("ds-icon-button");
    expect(screen.getByTestId("memory-delete-14").getAttribute("aria-label")).toBe("Delete this memory");
    expect(screen.getByRole("textbox", { name: "Your instructions" }).className).toContain("ds-field");
    expect(list.textContent).toContain("Prefers metric units");
    expect(acceptMemoryCandidate).not.toHaveBeenCalled();
    expect(deleteAllMemory).not.toHaveBeenCalled();
  });

  it("retains a pending suggestion and announces an error when Runtime rejects acceptance", async () => {
    vi.mocked(acceptMemoryCandidate).mockResolvedValue({ ok: false, error: "not persisted" });
    view();
    fireEvent.click(await screen.findByTestId("memory-candidate-accept-23"));
    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.getByTestId("memory-candidate-23")).toBeTruthy();
    expect(getMemoryCandidates).toHaveBeenCalledTimes(1);
    expect(acceptMemoryCandidate).toHaveBeenCalledWith(23, undefined);
  });

  it("blocks duplicate pending candidate decisions until the first one resolves", async () => {
    let finish!: (result: Awaited<ReturnType<typeof acceptMemoryCandidate>>) => void;
    vi.mocked(acceptMemoryCandidate).mockImplementation(() =>
      new Promise((resolve) => { finish = resolve; }),
    );
    view();
    const accept = await screen.findByTestId("memory-candidate-accept-23") as HTMLButtonElement;
    fireEvent.click(accept);
    expect(accept.disabled).toBe(true);
    expect(accept.getAttribute("aria-busy")).toBe("true");
    expect((screen.getByTestId("memory-candidate-decline-23") as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(accept);
    expect(acceptMemoryCandidate).toHaveBeenCalledTimes(1);
    await act(async () => { finish({ ok: true, memory_id: 17 }); });
    await waitFor(() => expect(getMemoryCandidates).toHaveBeenCalledTimes(2));
  });

  it("keeps memory editing open when an update fails and saves only after confirmation", async () => {
    vi.mocked(updateMemory).mockResolvedValueOnce({ ok: false, error: "read only" })
      .mockResolvedValueOnce({ ok: true, id: 15 });
    view();
    fireEvent.click(await screen.findByTestId("memory-edit-btn-14"));
    const editor = screen.getByRole("textbox", { name: "Fix this" }) as HTMLTextAreaElement;
    expect(editor.className).toContain("ds-field");
    fireEvent.change(editor, { target: { value: "  Metric and SI units  " } });
    fireEvent.click(screen.getByTestId("memory-save-14"));
    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.getByTestId("memory-edit-14")).toBeTruthy();
    expect(getMemory).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByTestId("memory-save-14"));
    await waitFor(() => expect(updateMemory).toHaveBeenLastCalledWith(14, "Metric and SI units"));
    await waitFor(() => expect(getMemory).toHaveBeenCalledTimes(2));
  });

  it("does not refresh or claim success when a memory deletion fails", async () => {
    vi.mocked(deleteMemory).mockResolvedValue({ ok: false, error: "disk error" });
    view();
    fireEvent.click(await screen.findByTestId("memory-delete-14"));
    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.getByTestId("memory-row-14")).toBeTruthy();
    expect(getMemory).toHaveBeenCalledTimes(1);
  });

  it("requires confirmation and a successful Runtime result before reporting delete-all success", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    view();
    const wipe = await screen.findByTestId("memory-delete-all");
    fireEvent.click(wipe);
    expect(deleteAllMemory).not.toHaveBeenCalled();
    confirm.mockReturnValue(true);
    vi.mocked(deleteAllMemory).mockResolvedValueOnce({ ok: false, deleted: 0 })
      .mockResolvedValueOnce({ ok: true, deleted: 1 });
    fireEvent.click(wipe);
    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.getByTestId("memory-list-msg").textContent).not.toContain("Everything");
    expect(getMemory).toHaveBeenCalledTimes(1);
    fireEvent.click(wipe);
    await waitFor(() => expect(screen.getByTestId("memory-list-msg").textContent).toContain("Everything"));
    await waitFor(() => expect(getMemory).toHaveBeenCalledTimes(2));
    expect(confirm).toHaveBeenCalledTimes(3);
  });

  it("keeps user rules unsaved after failure and permits an explicit retry", async () => {
    vi.mocked(setMemorySettings).mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValueOnce({ enabled: true, user_rules: "Evidence only" });
    view();
    const editor = await screen.findByRole("textbox", { name: "Your instructions" });
    fireEvent.change(editor, { target: { value: "Evidence only" } });
    const save = screen.getByTestId("user-rules-save") as HTMLButtonElement;
    expect(save.className).toContain("ds-button-primary");
    expect(save.disabled).toBe(false);
    fireEvent.click(save);
    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.queryByText("Saved on this computer. Not applied to conversations yet.")).toBeNull();
    fireEvent.click(save);
    await waitFor(() => expect(setMemorySettings).toHaveBeenLastCalledWith({ user_rules: "Evidence only" }));
    expect(await screen.findByRole("status")).toBeTruthy();
  });

  it("imports a selected file and explicitly exports via the shared actions", async () => {
    view();
    await screen.findByTestId("memory-list-card");
    const file = new File(['{"format":"delta.memory","version":1,"memories":[]}'], "memories.json", {
      type: "application/json",
    });
    Object.defineProperty(file, "text", { value: async () => '{"format":"delta.memory","version":1,"memories":[]}' });
    fireEvent.change(screen.getByTestId("memory-import-input"), { target: { files: [file] } });
    await waitFor(() => expect(importMemory).toHaveBeenCalledWith({
      format: "delta.memory", version: 1, memories: [],
    }));
    expect(screen.getByTestId("memory-list-msg").textContent).toContain("Imported 1 memories");
    const create = vi.fn(() => "blob:memory-test");
    const revoke = vi.fn();
    Object.defineProperty(URL, "createObjectURL", { configurable: true, value: create });
    Object.defineProperty(URL, "revokeObjectURL", { configurable: true, value: revoke });
    const click = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
    fireEvent.click(screen.getByTestId("memory-export"));
    await waitFor(() => expect(exportMemory).toHaveBeenCalledTimes(1));
    expect(create).toHaveBeenCalledTimes(1);
    expect(revoke).toHaveBeenCalledWith("blob:memory-test");
    expect(click).toHaveBeenCalled();
  });
});
