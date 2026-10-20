import type { ComponentProps } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { Sidebar } from "./Sidebar";
import { I18nProvider } from "@delta/i18n/I18nContext";
import type { SessionInfo } from "../types";

const api = vi.hoisted(() => ({
  getAutomations: vi.fn().mockResolvedValue([]),
  getEgressSettings: vi.fn().mockResolvedValue({ default_level: "open", is_chosen: true, workspaces: [], endpoint_marks: [], local_only_servers: [], path_overrides: [] }),
}));

vi.mock("../api", () => ({
  AUTOMATIONS_CHANGED: "delta:automations-changed",
  getAutomations: api.getAutomations,
  getEgressSettings: api.getEgressSettings,
}));

function egressLevel(level: string | null, isChosen = true) {
  return { default_level: level, is_chosen: isChosen, workspaces: [], endpoint_marks: [], local_only_servers: [], path_overrides: [] };
}

const SESSIONS: SessionInfo[] = [
  {
    session_id: "s-research-doe",
    title: "DOE — factor screening",
    workspace: "C:/Delta/s-research-doe",
    agent: "delta",
    model: "m",
    mode: "interactive",
    updated_at: "2026-07-01",
    messages: 4,
  },
  {
    session_id: "s-office-batch",
    title: "batch rename 100 images",
    workspace: "C:/Delta/s-office-batch",
    agent: "delta",
    model: "m",
    mode: "interactive",
    updated_at: "2026-06-29",
    messages: 2,
  },
];

const baseProps = {
  sessions: SESSIONS,
  activeSession: "s-research-doe",
  onNewSession: vi.fn(),
  onSelectSession: vi.fn(),
  onRenameSession: vi.fn(),
  onDeleteSession: vi.fn(),
  onArchiveSession: vi.fn(),
  onTogglePin: vi.fn(),
  onSetReasoningEffort: vi.fn(),
  onManage: vi.fn(),
  onOpenScheduled: vi.fn(),
  onOpenAutomation: vi.fn(),
  onOpenIntegrations: vi.fn(),
  onOpenAudit: vi.fn(),
  onOpenInbox: vi.fn(),
  scheduledActive: false,
  integrationsActive: false,
  auditActive: false,
  inboxActive: false,
};

function renderSidebar(props: ComponentProps<typeof Sidebar> = baseProps) {
  return render(
    <I18nProvider locale="en-US">
      <Sidebar {...props} />
    </I18nProvider>,
  );
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  api.getAutomations.mockResolvedValue([]);
});

describe("Delta task sidebar", () => {
  it("starts and selects Delta tasks without an identity picker", () => {
    renderSidebar();

    fireEvent.click(screen.getByText("New task"));
    expect(baseProps.onNewSession).toHaveBeenCalledOnce();

    fireEvent.click(screen.getByText("batch rename 100 images"));
    expect(baseProps.onSelectSession).toHaveBeenCalledWith(
      "s-office-batch",
      "C:/Delta/s-office-batch",
    );
  });

  it("keeps rename, pin, archive, reasoning and confirmed delete behind the row menu", () => {
    renderSidebar();
    const openMenu = () => fireEvent.click(screen.getAllByTestId("row-menu")[0]);

    openMenu();
    fireEvent.click(screen.getByTestId("row-menu-rename"));
    const input = screen.getByDisplayValue("DOE — factor screening");
    fireEvent.change(input, { target: { value: "factor screening final" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(baseProps.onRenameSession).toHaveBeenCalledWith(
      "s-research-doe",
      "factor screening final",
    );

    openMenu();
    fireEvent.click(screen.getByTestId("row-menu-pin"));
    expect(baseProps.onTogglePin).toHaveBeenCalledWith("s-research-doe", true);

    openMenu();
    fireEvent.click(screen.getByTestId("row-menu-archive"));
    expect(baseProps.onArchiveSession).toHaveBeenCalledWith("s-research-doe", true);

    openMenu();
    fireEvent.click(screen.getByRole("menuitemradio", { name: "Deep" }));
    expect(baseProps.onSetReasoningEffort).toHaveBeenCalledWith("s-research-doe", "high");

    openMenu();
    fireEvent.click(screen.getByTestId("row-menu-delete"));
    expect(baseProps.onDeleteSession).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTestId("row-menu-delete"));
    expect(baseProps.onDeleteSession).toHaveBeenCalledWith("s-research-doe");
  });

  it("renders Slack origin inside Recent rather than creating an identity band", () => {
    const slack: SessionInfo = {
      ...SESSIONS[0],
      session_id: "s-slack-1",
      title: "#general — check the deploy?",
      updated_at: "2026-07-13",
      origin: "slack",
      origin_label: "#general · T0AB",
    };
    renderSidebar({ ...baseProps, sessions: [...SESSIONS, slack] });

    const row = screen.getByText("#general — check the deploy?").closest(".group");
    expect(row?.querySelector('[data-logo="slack"]')).toBeTruthy();
    expect(screen.queryByTestId("from-slack-toggle")).toBeNull();
  });
});

describe("Delta primary nav group (design 2:615)", () => {
  beforeEach(() => {
    api.getAutomations.mockResolvedValue([]);
    api.getEgressSettings.mockResolvedValue(egressLevel("private_only"));
  });

  it("lists the six primary destinations inside one navigation landmark", () => {
    renderSidebar(baseProps);
    const nav = screen.getByRole("navigation", { name: "Primary" });
    const names = within(nav).getAllByRole("button").map((b) => b.textContent);
    expect(names).toEqual(["Conversations", "Task center", "Automations", "Inbox", "Connectors", "Audit"]);
  });

  it("routes each destination to its surface", () => {
    const props = { ...baseProps, onOpenConversations: vi.fn(), onOpenPrivacy: vi.fn() };
    renderSidebar(props);
    fireEvent.click(screen.getByTestId("nav-conversations"));
    expect(props.onOpenConversations).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByTestId("nav-automations"));
    expect(baseProps.onOpenScheduled).toHaveBeenCalled();
    fireEvent.click(screen.getByTestId("nav-inbox"));
    expect(baseProps.onOpenInbox).toHaveBeenCalled();
    fireEvent.click(screen.getByTestId("nav-connectors"));
    expect(baseProps.onOpenIntegrations).toHaveBeenCalled();
    fireEvent.click(screen.getByTestId("nav-audit"));
    expect(baseProps.onOpenAudit).toHaveBeenCalled();
  });

  it("marks the active destination with aria-current and the selected tokens", () => {
    renderSidebar({ ...baseProps, conversationActive: false, auditActive: true });
    expect(screen.getByTestId("nav-audit").getAttribute("aria-current")).toBe("page");
    expect(screen.getByTestId("nav-audit").className).toContain("bg-selected");
    expect(screen.getByTestId("nav-conversations").getAttribute("aria-current")).toBeNull();
  });

  it("shows the attention total as the Inbox chip, without a separate fetch", () => {
    const sessions = [{ ...SESSIONS[0], attention: 3 }, SESSIONS[1]];
    renderSidebar({ ...baseProps, sessions });
    expect(within(screen.getByTestId("nav-inbox")).getByText("3")).toBeTruthy();
  });

  it("shows the default egress level in the trust row and opens Privacy on click", async () => {
    const props = { ...baseProps, onOpenPrivacy: vi.fn() };
    renderSidebar(props);
    const trust = await screen.findByText("Local tools · Private only");
    fireEvent.click(trust.closest("button")!);
    expect(props.onOpenPrivacy).toHaveBeenCalledTimes(1);
  });

  it("says no level is chosen when the egress setting is not yet made", async () => {
    api.getEgressSettings.mockResolvedValue(egressLevel(null, false));
    renderSidebar(baseProps);
    expect(await screen.findByText("Local tools · No level chosen")).toBeTruthy();
  });

  it("the task center entry returns to the conversation view and the top of the task list", () => {
    const props = { ...baseProps, onOpenConversations: vi.fn() };
    renderSidebar(props);
    fireEvent.click(screen.getByTestId("nav-task-center"));
    expect(props.onOpenConversations).toHaveBeenCalledTimes(1);
  });

  it("shows the local device in the user row", () => {
    renderSidebar(baseProps);
    expect(within(screen.getByTestId("nav-user")).getByText("Local")).toBeTruthy();
  });
});
