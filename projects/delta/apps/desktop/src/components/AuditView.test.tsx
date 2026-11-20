// Audit header (design 2:603): three counts over the loaded records, and "View source" opens the
// conversation a record came from.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { AuditView } from "./AuditView";
import { I18nProvider } from "@delta/i18n/I18nContext";
import type { AuditEvent } from "../api";

const { getAuditMock } = vi.hoisted(() => ({ getAuditMock: vi.fn() }));

vi.mock("../api", async (importOriginal) => {
  const original = await importOriginal<typeof import("../api")>();
  return {
    ...original,
    getAudit: (...a: unknown[]) => getAuditMock(...a),
  };
});

const event = (over: Partial<AuditEvent>): AuditEvent => ({
  id: 1,
  timestamp: "2026-10-20T09:00:00Z",
  session_id: "s-1",
  agent: "delta",
  workspace: "/ws",
  connector: "",
  tool: "web_search",
  stage: "done",
  status: "done",
  approval: "",
  args: {},
  result_preview: "",
  reason: "",
  resource: "",
  ...over,
});

const wrap = (onOpenSession?: (id: string) => void) => (
  <I18nProvider locale="en-US">
    <AuditView onOpenSession={onOpenSession} />
  </I18nProvider>
);

describe("Audit header and source link", () => {
  afterEach(cleanup);

  it("shows the total, waiting and failed counts for the loaded records", async () => {
    getAuditMock.mockResolvedValue([
      event({ id: 1 }),
      event({ id: 2, status: "pending", stage: "approval_required" }),
      event({ id: 3, status: "failed", stage: "failed" }),
    ]);
    render(wrap());
    const stats = await screen.findByTestId("audit-stats");
    const counts = within(stats).getAllByText(/^\d+$/).map((el) => el.textContent);
    expect(counts).toEqual(["3", "1", "1"]);
  });

  it("View source opens the conversation the record came from", async () => {
    getAuditMock.mockResolvedValue([event({ session_id: "s-42" })]);
    const onOpenSession = vi.fn();
    render(wrap(onOpenSession));
    const source = await screen.findByRole("button", { name: "View source: s-42" });
    expect(source.className).toContain("ds-button-ghost");
    expect(source.getAttribute("type")).toBe("button");
    fireEvent.click(source);
    expect(onOpenSession).toHaveBeenCalledWith("s-42");
  });

  it("has no source link when the page cannot open a conversation", async () => {
    getAuditMock.mockResolvedValue([event({})]);
    render(wrap());
    await screen.findByTestId("audit-stats");
    expect(screen.queryByRole("button", { name: /View source/ })).toBeNull();
  });
});


describe("activity shared filter controls", () => {
  beforeEach(() => getAuditMock.mockReset());
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("filters only on explicit submit, trims each value, and supports native Enter submission", async () => {
    getAuditMock.mockResolvedValue([event({})]);
    render(wrap());

    await screen.findByTestId("audit-stats");
    expect(getAuditMock).toHaveBeenCalledTimes(1);
    const session = screen.getByRole("textbox", { name: "Conversation (optional)" });
    const connector = screen.getByRole("textbox", { name: "App (optional)" });
    const tool = screen.getByRole("textbox", { name: "Action (optional)" });
    for (const input of [session, connector, tool]) expect(input.className).toContain("ds-field");

    fireEvent.change(session, { target: { value: "  s-42  " } });
    fireEvent.change(connector, { target: { value: "  browser " } });
    fireEvent.change(tool, { target: { value: " web_search  " } });
    expect(getAuditMock).toHaveBeenCalledTimes(1);

    const submit = screen.getByRole("button", { name: "Filter" });
    expect(submit.className).toContain("ds-button-primary");
    expect(submit.getAttribute("type")).toBe("submit");
    fireEvent.click(submit);
    await waitFor(() => expect(getAuditMock).toHaveBeenLastCalledWith({
      limit: 150, session_id: "s-42", connector: "browser", tool: "web_search",
    }));

    fireEvent.change(session, { target: { value: "s-88" } });
    fireEvent.submit(screen.getByTestId("audit-filters"));
    await waitFor(() => expect(getAuditMock).toHaveBeenLastCalledWith({
      limit: 150, session_id: "s-88", connector: "browser", tool: "web_search",
    }));
  });

  it("keeps technical fields behind the native disclosure and leaves audit data read-only", async () => {
    getAuditMock.mockResolvedValue([event({ args: { query: "release status" } })]);
    render(wrap());
    const summary = await screen.findByText("Technical details");
    const details = summary.closest("details") as HTMLDetailsElement;
    expect(details).toBeTruthy();
    expect(details.open).toBe(false);
    expect(within(details).getByText(/query=release status/)).toBeTruthy();
    fireEvent.click(summary);
    expect(details.open).toBe(true);
    expect(screen.queryByRole("button", { name: /View source/ })).toBeNull();
  });

  it("distinguishes a failed fetch from an empty audit log and allows retry", async () => {
    getAuditMock.mockRejectedValueOnce(new Error("offline")).mockResolvedValueOnce([]);
    render(wrap());
    expect(await screen.findByTestId("load-error")).toBeTruthy();
    expect(screen.queryByText("No audit events yet.")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("No audit events yet.")).toBeTruthy();
    expect(getAuditMock).toHaveBeenCalledTimes(2);
  });
});
