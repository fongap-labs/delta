// Audit header (design 2:603): three counts over the loaded records, and "View source" opens the
// conversation a record came from.
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
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
    fireEvent.click(await screen.findByRole("button", { name: "View source" }));
    expect(onOpenSession).toHaveBeenCalledWith("s-42");
  });

  it("has no source link when the page cannot open a conversation", async () => {
    getAuditMock.mockResolvedValue([event({})]);
    render(wrap());
    await screen.findByTestId("audit-stats");
    expect(screen.queryByRole("button", { name: "View source" })).toBeNull();
  });
});
