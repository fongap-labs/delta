// Governance rail: the Permissions section lists this session's approval decisions and the
// workspace egress level; the footer points to the full record in Audit (design 2:1493).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import type { JSX } from "react";
import { RightRail } from "./RightRail";
import { I18nProvider } from "@delta/i18n/I18nContext";
import type { Item } from "../types";

const { getArtifactsMock, getEgressSettingsMock } = vi.hoisted(() => ({
  getArtifactsMock: vi.fn(),
  getEgressSettingsMock: vi.fn(),
}));

vi.mock("../api", async (importOriginal) => {
  const original = await importOriginal<typeof import("../api")>();
  return {
    ...original,
    getArtifacts: (...a: unknown[]) => getArtifactsMock(...a),
    getEgressSettings: (...a: unknown[]) => getEgressSettingsMock(...a),
  };
});

type Approval = Extract<Item, { kind: "approval" }>;

const granted: Approval = { kind: "approval", name: "write_file", args: { path: "notes.md" }, reason: "", resolved: "once" };
const pending: Approval = { kind: "approval", name: "run_shell", args: { command: "npm test" }, reason: "" };
const revoked: Approval = { kind: "approval", name: "send_message", args: { target: "slack:#general" }, reason: "", resolved: "deny" };

const props = (extra: Record<string, unknown> = {}) => ({
  active: true,
  sessionId: "s1",
  refreshKey: 0,
  toolNames: [],
  todo: [] as never[],
  running: false,
  showArtifacts: true,
  ...extra,
});

const wrap = (el: JSX.Element) => <I18nProvider locale="en-US">{el}</I18nProvider>;

describe("RightRail permissions section and footer", () => {
  afterEach(cleanup);

  beforeEach(() => {
    getArtifactsMock.mockResolvedValue([]);
    getEgressSettingsMock.mockResolvedValue({ default_level: "ask_each_time", is_chosen: true, workspaces: [], endpoint_marks: [], local_only_servers: [], path_overrides: [] });
  });

  it("sits between Progress and Artifacts", () => {
    render(wrap(<RightRail {...props()} />));
    const titles = Array.from(document.querySelectorAll(".rail-section-toggle")).map((el) => el.textContent);
    expect(titles.indexOf("Permissions")).toBeGreaterThan(titles.indexOf("Progress"));
    expect(titles.indexOf("Permissions")).toBeLessThan(titles.findIndex((x) => x?.startsWith("Artifacts")));
  });

  it("shows the empty line when this session has no approval requests", async () => {
    render(wrap(<RightRail {...props()} />));
    expect(await screen.findByText("No permission requests in this conversation yet.")).toBeTruthy();
  });

  it("shows the workspace egress level as the first row", async () => {
    render(wrap(<RightRail {...props()} />));
    const row = await screen.findByTestId("rail-perm-egress");
    expect(await within(row).findByText("Ask each time")).toBeTruthy();
  });

  it("gives each decision one row with a state, a name and a chip", async () => {
    render(wrap(<RightRail {...props({ approvals: [granted, pending, revoked] })} />));
    const rows = await screen.findAllByTestId("rail-perm-row");
    expect(rows.map((r) => r.getAttribute("data-state"))).toEqual(["granted", "pending", "revoked"]);
    expect(within(rows[0]).getByText("This session")).toBeTruthy();
    expect(within(rows[1]).getByText("Needs approval")).toBeTruthy();
    expect(within(rows[2]).getByText("Revoked")).toBeTruthy();
  });

  it("a pending row scrolls the approval card into view", async () => {
    const card = document.createElement("div");
    card.className = "approval";
    card.scrollIntoView = vi.fn();
    document.body.appendChild(card);
    render(wrap(<RightRail {...props({ approvals: [pending] })} />));
    fireEvent.click(await screen.findByRole("button", { name: /npm test|run/i }));
    expect(card.scrollIntoView).toHaveBeenCalled();
    card.remove();
  });

  it("folds Access into the Permissions section rather than a section of its own", async () => {
    render(wrap(<RightRail {...props()} />));
    const access = await screen.findByTestId("access-section");
    const section = access.closest("section");
    expect(section?.querySelector(".rail-section-toggle")?.textContent).toBe("Permissions");
    expect(screen.queryByTestId("access-toggle")).toBeNull();
    const titles = Array.from(document.querySelectorAll(".rail-section-toggle")).map((el) => el.textContent);
    expect(titles.filter((title) => title?.startsWith("Access"))).toEqual([]);
  });

  it("the footer opens Audit", async () => {
    const onOpenAudit = vi.fn();
    render(wrap(<RightRail {...props({ onOpenAudit })} />));
    fireEvent.click(await screen.findByTestId("rail-footer"));
    expect(onOpenAudit).toHaveBeenCalledTimes(1);
  });
});
