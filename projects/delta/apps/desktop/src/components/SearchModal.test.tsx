import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { I18nProvider } from "@delta/i18n/I18nContext";
import type { SessionInfo } from "../types";
import { SearchModal } from "./SearchModal";

const searchConversations = vi.fn();
vi.mock("../api", () => ({
  searchConversations: (...args: unknown[]) => searchConversations(...args),
}));

const session = (id: string, title: string): SessionInfo =>
  ({
    session_id: id,
    title,
    workspace: "/work/a",
    updated_at: "2026-10-26 09:00:00",
    pinned: false,
    archived: false,
  }) as unknown as SessionInfo;

const sessions = [session("s1", "Budget review"), session("s2", "Unrelated chat")];

function renderModal(overrides: Partial<Parameters<typeof SearchModal>[0]> = {}) {
  const props = {
    sessions,
    workspace: "/work/a",
    onSelect: vi.fn(),
    onClose: vi.fn(),
    ...overrides,
  };
  render(
    <I18nProvider locale="en-US">
      <SearchModal {...props} />
    </I18nProvider>,
  );
  return props;
}

describe("SearchModal message search", () => {
  beforeEach(() => {
    searchConversations.mockReset();
  });
  afterEach(cleanup);

  it("does not search messages until the query has two characters", () => {
    renderModal();
    fireEvent.change(screen.getByPlaceholderText("Search chats"), { target: { value: "b" } });
    expect(searchConversations).not.toHaveBeenCalled();
  });

  it("lists matching messages from the workspace, skipping sessions whose title already matched", async () => {
    searchConversations.mockResolvedValue([
      { session_id: "s1", title: "Budget review", line: 4, role: "user", snippet: "the quarterly budget" },
      { session_id: "s2", title: "Unrelated chat", line: 2, role: "assistant", snippet: "budget note" },
      { session_id: "s2", title: "Unrelated chat", line: 6, role: "user", snippet: "second budget note" },
    ]);
    renderModal();
    fireEvent.change(screen.getByPlaceholderText("Search chats"), { target: { value: "budget" } });

    await waitFor(() => expect(screen.getByText("In messages")).toBeTruthy());
    expect(searchConversations).toHaveBeenCalledWith("/work/a", "budget");
    expect(screen.getByText("budget note")).toBeTruthy();
    expect(screen.queryByText("second budget note")).toBeNull();
    expect(screen.getByText("line 2")).toBeTruthy();
  });

  it("opens the session of a message hit with its workspace", async () => {
    searchConversations.mockResolvedValue([
      { session_id: "s2", title: "Unrelated chat", line: 2, role: "assistant", snippet: "budget note" },
    ]);
    const props = renderModal();
    fireEvent.change(screen.getByPlaceholderText("Search chats"), { target: { value: "budget" } });

    const row = await screen.findByText("budget note");
    fireEvent.click(row.closest("button") as HTMLButtonElement);
    expect(props.onSelect).toHaveBeenCalledWith("s2", "/work/a");
    expect(props.onClose).toHaveBeenCalled();
  });

  it("reaches message rows with the arrow keys and opens them with Enter", async () => {
    searchConversations.mockResolvedValue([
      { session_id: "s2", title: "Unrelated chat", line: 2, role: "assistant", snippet: "budget note" },
    ]);
    const props = renderModal();
    const input = screen.getByPlaceholderText("Search chats");
    fireEvent.change(input, { target: { value: "budget" } });
    await screen.findByText("budget note");

    // The title match ("Budget review") is first; one ArrowDown reaches the message row.
    fireEvent.keyDown(input.closest("div.fixed") as HTMLElement, { key: "ArrowDown" });
    fireEvent.keyDown(input.closest("div.fixed") as HTMLElement, { key: "Enter" });
    expect(props.onSelect).toHaveBeenCalledWith("s2", "/work/a");
  });

  it("does not search messages when no workspace is open", () => {
    renderModal({ workspace: undefined });
    fireEvent.change(screen.getByPlaceholderText("Search chats"), { target: { value: "budget" } });
    expect(searchConversations).not.toHaveBeenCalled();
  });
});
