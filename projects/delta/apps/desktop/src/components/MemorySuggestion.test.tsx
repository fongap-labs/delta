import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@delta/i18n/I18nContext";
import { MemorySuggestion } from "./MemorySuggestion";
import {
  acceptMemoryCandidate,
  announceMemoryChanged,
  deleteMemory,
  getMemoryCandidates,
  rejectMemoryCandidate,
} from "../api";

vi.mock("../api", () => ({
  acceptMemoryCandidate: vi.fn(),
  announceMemoryChanged: vi.fn(),
  deleteMemory: vi.fn(),
  getMemoryCandidates: vi.fn(),
  rejectMemoryCandidate: vi.fn(),
}));

const id = 23;
function show() {
  return render(
    <I18nProvider locale="en-US">
      <MemorySuggestion candidateId={id} text="Remember preferred format" quote="Keep reports short" />
    </I18nProvider>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(getMemoryCandidates).mockResolvedValue([
    { id, content: "Remember preferred format" },
  ] as never);
  vi.mocked(acceptMemoryCandidate).mockResolvedValue({ ok: true, memory_id: 71 });
  vi.mocked(rejectMemoryCandidate).mockResolvedValue({ ok: true });
  vi.mocked(deleteMemory).mockResolvedValue({ ok: true });
});
afterEach(cleanup);

describe("memory suggestion shared controls", () => {
  it("keeps suggestions opt-in and records a decline only after an explicit action", async () => {
    show();
    const remember = await screen.findByTestId("memory-suggestion-accept");
    const decline = screen.getByTestId("memory-suggestion-decline");
    expect(remember.className).toContain("ds-button-primary");
    expect(decline.className).toContain("ds-button-ghost");
    expect(remember.getAttribute("type")).toBe("button");
    expect(screen.getByTestId("memory-suggestion-quote").textContent).toContain("Keep reports short");
    expect(acceptMemoryCandidate).not.toHaveBeenCalled();
    expect(rejectMemoryCandidate).not.toHaveBeenCalled();
    fireEvent.click(decline);
    await waitFor(() => expect(rejectMemoryCandidate).toHaveBeenCalledWith(id));
    expect((await screen.findByRole("status")).textContent).toContain("I won’t remember that");
    expect(acceptMemoryCandidate).not.toHaveBeenCalled();
    expect(announceMemoryChanged).toHaveBeenCalledTimes(1);
  });

  it("keeps editing local until Save, trimming text before the accepted note is stored", async () => {
    show();
    fireEvent.click(await screen.findByTestId("memory-suggestion-edit"));
    const editor = screen.getByRole("textbox", { name: "Edit" }) as HTMLTextAreaElement;
    expect(editor.className).toContain("ds-field");
    const save = screen.getByTestId("memory-suggestion-save") as HTMLButtonElement;
    expect(save.className).toContain("ds-button-primary");
    fireEvent.change(editor, { target: { value: "   " } });
    expect(save.disabled).toBe(true);
    fireEvent.change(editor, { target: { value: "   Short, cited reports   " } });
    expect(acceptMemoryCandidate).not.toHaveBeenCalled();
    fireEvent.click(save);
    await waitFor(() => expect(acceptMemoryCandidate).toHaveBeenCalledWith(id, "Short, cited reports"));
    expect(screen.getByTestId("memory-suggestion").textContent).toContain("Short, cited reports");
    expect(announceMemoryChanged).toHaveBeenCalledTimes(1);
  });

  it("prevents duplicate acceptance while the memory API is pending", async () => {
    let finish!: (value: Awaited<ReturnType<typeof acceptMemoryCandidate>>) => void;
    vi.mocked(acceptMemoryCandidate).mockImplementation(
      () => new Promise((resolve) => { finish = resolve; }),
    );
    show();
    const accept = await screen.findByTestId("memory-suggestion-accept") as HTMLButtonElement;
    fireEvent.click(accept);
    expect(accept.disabled).toBe(true);
    expect(accept.getAttribute("aria-busy")).toBe("true");
    expect((screen.getByTestId("memory-suggestion-decline") as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(accept);
    expect(acceptMemoryCandidate).toHaveBeenCalledTimes(1);
    expect(announceMemoryChanged).not.toHaveBeenCalled();
    await act(async () => { finish({ ok: true, memory_id: 71 }); });
    expect(screen.getByTestId("memory-suggestion-undo")).toBeTruthy();
    expect(announceMemoryChanged).toHaveBeenCalledTimes(1);
  });

  it("reports a failed acceptance without announcing that memory changed", async () => {
    vi.mocked(acceptMemoryCandidate).mockRejectedValue(new Error("disk unavailable"));
    show();
    fireEvent.click(await screen.findByTestId("memory-suggestion-accept"));
    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.queryByTestId("memory-suggestion-undo")).toBeNull();
    expect(announceMemoryChanged).not.toHaveBeenCalled();
  });

  it("reports a failed undo instead of claiming the note was forgotten", async () => {
    vi.mocked(deleteMemory).mockResolvedValue({ ok: false, error: "storage unavailable" });
    show();
    fireEvent.click(await screen.findByTestId("memory-suggestion-accept"));
    fireEvent.click(await screen.findByTestId("memory-suggestion-undo"));
    await waitFor(() => expect(deleteMemory).toHaveBeenCalledWith(71));
    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.queryByTestId("memory-suggestion-undone")).toBeNull();
    expect(announceMemoryChanged).toHaveBeenCalledTimes(1);
  });

  it("shows successful Undo only after the saved note was actually deleted", async () => {
    show();
    fireEvent.click(await screen.findByTestId("memory-suggestion-accept"));
    const undo = await screen.findByTestId("memory-suggestion-undo");
    expect(undo.className).toContain("ds-button");
    fireEvent.click(undo);
    await waitFor(() => expect(deleteMemory).toHaveBeenCalledWith(71));
    expect(await screen.findByTestId("memory-suggestion-undone")).toBeTruthy();
    expect(announceMemoryChanged).toHaveBeenCalledTimes(2);
  });
});
