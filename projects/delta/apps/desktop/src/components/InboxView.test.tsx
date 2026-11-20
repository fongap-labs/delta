import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { I18nProvider } from "@delta/i18n/I18nContext";
import { InboxView } from "./InboxView";
import {
  getConnectors, getInbox, getInboxRouting, getRecentChannels,
  getUnrouted, resolveInboxItem, type InboxItem,
} from "../api";

vi.mock("../api", () => ({
  getConnectors: vi.fn(), getInbox: vi.fn(), getInboxRouting: vi.fn(),
  getRecentChannels: vi.fn(), getUnrouted: vi.fn(), resolveInboxItem: vi.fn(),
}));
vi.mock("./InboxConfigure", () => ({
  InboxConfigure: () => <div data-testid="inbox-configure">Routing settings</div>,
}));
vi.mock("./InboxItemCard", () => ({
  InboxItemCard: ({ item }: { item: InboxItem }) => (
    <div data-testid={`inbox-item-${item.id}`}>{item.title}</div>
  ),
}));
vi.mock("./IntegrationsView", () => ({
  PanelHead: ({ title }: { title: string }) => <h1>{title}</h1>,
}));

const item = (id: string, kind: InboxItem["kind"], session: string, tool?: string): InboxItem => ({
  id, kind, session_id: session, title: id, body: "", state: "pending",
  resolution: null, inbox: "default", created_at: "", resolved_at: null,
  session_title: session, session_exists: true, data: tool ? { tool, arguments: {} } : undefined,
});
const ITEMS = [
  item("a1", "approval", "task-a", "write_file"),
  item("a2", "approval", "task-a", "web_fetch"),
  item("q1", "question", "task-b"),
];

function view() {
  return render(
    <I18nProvider locale="en-US">
      <InboxView onOpenSession={vi.fn()} onOpenArtifact={vi.fn()} />
    </I18nProvider>,
  );
}

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(getInbox).mockResolvedValue(ITEMS);
  vi.mocked(getInboxRouting).mockResolvedValue([]);
  vi.mocked(getConnectors).mockResolvedValue([]);
  vi.mocked(getRecentChannels).mockResolvedValue([]);
  vi.mocked(getUnrouted).mockResolvedValue([]);
  vi.mocked(resolveInboxItem).mockResolvedValue({ ok: true });
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

describe("Inbox shared navigation and resolution controls", () => {
  it("links page tabs to one panel and moves selected focus via Arrow, Home and End", async () => {
    view();
    const pending = screen.getByRole("tab", { name: /Pending/ });
    const configure = screen.getByRole("tab", { name: /Configure/ });
    expect(pending.className).toContain("ds-button");
    expect(pending.getAttribute("type")).toBe("button");
    expect(pending.getAttribute("aria-selected")).toBe("true");
    expect(pending.getAttribute("tabindex")).toBe("0");
    expect(configure.getAttribute("tabindex")).toBe("-1");
    expect(pending.getAttribute("aria-controls")).toBe("inbox-panel");
    await screen.findByTestId("inbox-item-a1");
    pending.focus();
    fireEvent.keyDown(pending, { key: "ArrowRight" });
    expect(document.activeElement).toBe(configure);
    expect(configure.getAttribute("aria-selected")).toBe("true");
    expect(screen.getByRole("tabpanel", { name: /Configure/ })).toBeTruthy();
    expect(screen.getByTestId("inbox-configure")).toBeTruthy();
    fireEvent.keyDown(configure, { key: "Home" });
    expect(document.activeElement).toBe(pending);
    expect(screen.getByRole("tabpanel", { name: /Pending/ })).toBeTruthy();
    fireEvent.keyDown(pending, { key: "End" });
    expect(document.activeElement).toBe(configure);
    fireEvent.keyDown(configure, { key: "ArrowLeft" });
    expect(document.activeElement).toBe(pending);
    expect(resolveInboxItem).not.toHaveBeenCalled();
  });

  it("filters by kind with pressed state without resolving a pending item", async () => {
    view();
    await screen.findByTestId("inbox-item-a1");
    const filters = screen.getByTestId("inbox-filters");
    const approvals = filters.querySelector<HTMLButtonElement>('button[aria-pressed="false"]');
    expect(approvals).not.toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Questions" }));
    expect(screen.getByRole("button", { name: "Questions" }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByTestId("inbox-item-q1")).toBeTruthy();
    expect(screen.queryByTestId("inbox-item-a1")).toBeNull();
    expect(resolveInboxItem).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "All" }));
    expect(screen.getByTestId("inbox-item-a1")).toBeTruthy();
  });

  it("requires explicit confirmation before allowing several approvals", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    view();
    const allow = await screen.findByTestId("inbox-allow-all");
    expect(allow.className).toContain("ds-button");
    expect(allow.getAttribute("type")).toBe("button");
    fireEvent.click(allow);
    expect(confirm).toHaveBeenCalledTimes(1);
    expect(confirm.mock.calls[0]?.[0]).toContain("write_file, web_fetch");
    expect(resolveInboxItem).not.toHaveBeenCalled();

    confirm.mockReturnValue(true);
    fireEvent.click(allow);
    await waitFor(() => expect(resolveInboxItem).toHaveBeenCalledTimes(2));
    expect(resolveInboxItem).toHaveBeenNthCalledWith(1, "a1", "allow");
    expect(resolveInboxItem).toHaveBeenNthCalledWith(2, "a2", "allow");
    expect(resolveInboxItem).not.toHaveBeenCalledWith("q1", "allow");
  });

  it("denies only the matching task approvals without affirmative confirmation", async () => {
    const confirm = vi.spyOn(window, "confirm");
    view();
    fireEvent.click(await screen.findByTestId("inbox-deny-all"));
    await waitFor(() => expect(resolveInboxItem).toHaveBeenCalledTimes(2));
    expect(resolveInboxItem).toHaveBeenNthCalledWith(1, "a1", "deny");
    expect(resolveInboxItem).toHaveBeenNthCalledWith(2, "a2", "deny");
    expect(confirm).not.toHaveBeenCalled();
  });
});
