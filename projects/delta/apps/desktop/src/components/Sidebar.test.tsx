import type { ComponentProps } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { Sidebar } from "./Sidebar";
import { I18nProvider } from "@delta/i18n/I18nContext";
import type { SessionInfo } from "../types";

const api = vi.hoisted(() => ({
  getAutomations: vi.fn().mockResolvedValue([]),
}));

vi.mock("../api", () => ({
  AUTOMATIONS_CHANGED: "delta:automations-changed",
  getAutomations: api.getAutomations,
}));

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
  it("selects Delta tasks without an identity picker", () => {
    renderSidebar();

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

describe("Delta sidebar layout", () => {
  beforeEach(() => {
    api.getAutomations.mockResolvedValue([]);
  });

  it("puts the four tool buttons in one top row", () => {
    renderSidebar({ ...baseProps });
    const row = screen.getByTestId("sidebar-footer-inbox").parentElement;
    for (const testid of [
      "sidebar-footer-inbox",
      "sidebar-footer-activity",
      "sidebar-footer-integrations",
      "sidebar-footer-settings",
    ]) {
      expect(row?.contains(screen.getByTestId(testid))).toBe(true);
    }
  });

  it("lists the scheduled-task and conversation destinations, then the search field", () => {
    renderSidebar(baseProps);
    const search = screen.getByTestId("nav-search");
    const nav = screen.getByRole("navigation", { name: "Primary" });
    const names = within(nav).getAllByRole("button").map((b) => b.textContent);
    expect(names).toEqual(["Scheduled tasks", "Start chat"]);
    expect(nav.compareDocumentPosition(search) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("routes each destination to its surface", () => {
    const props = { ...baseProps, onNewSession: vi.fn(), onManage: vi.fn() };
    renderSidebar(props);
    fireEvent.click(screen.getByTestId("nav-conversations"));
    expect(props.onNewSession).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByTestId("nav-automations"));
    expect(baseProps.onOpenScheduled).toHaveBeenCalled();
    fireEvent.click(screen.getByTestId("sidebar-footer-inbox"));
    expect(baseProps.onOpenInbox).toHaveBeenCalled();
    fireEvent.click(screen.getByTestId("sidebar-footer-integrations"));
    expect(baseProps.onOpenIntegrations).toHaveBeenCalled();
    fireEvent.click(screen.getByTestId("sidebar-footer-activity"));
    expect(baseProps.onOpenAudit).toHaveBeenCalled();
    fireEvent.click(screen.getByTestId("sidebar-footer-settings"));
    expect(props.onManage).toHaveBeenCalled();
  });

  it("marks the active destination with aria-current and aria-pressed", () => {
    renderSidebar({ ...baseProps, conversationActive: true, auditActive: true });
    expect(screen.getByTestId("nav-conversations").getAttribute("aria-current")).toBe("page");
    expect(screen.getByTestId("sidebar-footer-activity").getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByTestId("nav-automations").getAttribute("aria-current")).toBeNull();
  });

  it("shows the attention total on the Inbox button, without a separate fetch", () => {
    const sessions = [{ ...SESSIONS[0], attention: 3 }, SESSIONS[1]];
    renderSidebar({ ...baseProps, sessions });
    expect(within(screen.getByTestId("sidebar-footer-inbox")).getByText("3")).toBeTruthy();
  });

  it("no longer carries the task-center, connectors/audit rows, trust row, new-task button or user row", () => {
    renderSidebar(baseProps);
    expect(screen.queryByText("New task")).toBeNull();
    expect(screen.queryByTestId("nav-task-center")).toBeNull();
    expect(screen.queryByTestId("nav-inbox")).toBeNull();
    expect(screen.queryByTestId("nav-connectors")).toBeNull();
    expect(screen.queryByTestId("nav-audit")).toBeNull();
    expect(screen.queryByTestId("nav-trust")).toBeNull();
    expect(screen.queryByTestId("nav-user")).toBeNull();
  });

  it("offers a left-edge resize handle only while the sidebar is docked", () => {
    const { unmount } = renderSidebar({ ...baseProps, onResize: vi.fn(), resizeMaxWidth: () => 600 });
    expect(screen.getByRole("separator", { name: "Resize left sidebar" })).toBeTruthy();
    unmount();

    renderSidebar({ ...baseProps, onResize: vi.fn(), resizeMaxWidth: () => 600, collapsed: true });
    expect(screen.queryByRole("separator")).toBeNull();
  });
});


describe("Delta sidebar project controls", () => {
  beforeEach(() => localStorage.removeItem("delta:projects:v1"));
  afterEach(() => localStorage.removeItem("delta:projects:v1"));

  it("creates, renames and collapses a project with shared accessible fields", () => {
    renderSidebar();
    const create = screen.getByTestId("new-project");
    expect(create.className).toContain("ds-icon-button");
    fireEvent.click(create);
    const name = screen.getByRole("textbox", { name: "Project name" }) as HTMLInputElement;
    expect(name.className).toContain("ds-field");
    fireEvent.change(name, { target: { value: "Quality review" } });
    fireEvent.keyDown(name, { key: "Enter" });

    const project = screen.getByTestId("projects-band");
    const toggle = within(project).getByRole("button", { name: "Collapse Quality review" });
    expect(toggle.className).toContain("ds-button-ghost");
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    fireEvent.click(toggle);
    expect(within(project).getByRole("button", { name: "Expand Quality review" }).getAttribute("aria-expanded")).toBe("false");

    const rename = within(project).getByRole("button", { name: "Rename: Quality review" });
    expect(rename.className).toContain("ds-icon-button");
    fireEvent.click(rename);
    const edit = screen.getByRole("textbox", { name: "Rename: Quality review" }) as HTMLInputElement;
    expect(edit.className).toContain("ds-field");
    fireEvent.change(edit, { target: { value: "Quality governance" } });
    fireEvent.keyDown(edit, { key: "Enter" });
    expect(within(project).getByRole("button", { name: "Expand Quality governance" })).toBeTruthy();
  });

  it("requires two explicit actions to remove a project and never deletes tasks", () => {
    renderSidebar();
    fireEvent.click(screen.getByTestId("new-project"));
    const name = screen.getByRole("textbox", { name: "Project name" });
    fireEvent.change(name, { target: { value: "Engineering" } });
    fireEvent.keyDown(name, { key: "Enter" });

    const project = screen.getByTestId("projects-band");
    let remove = within(project).getByRole("button", { name: "Remove project: Engineering" });
    expect(remove.className).toContain("ds-button-danger");
    fireEvent.click(remove);
    expect(within(project).getByRole("button", { name: "Remove? Tasks are kept.: Engineering" })).toBeTruthy();
    expect(baseProps.onDeleteSession).not.toHaveBeenCalled();
    remove = within(project).getByRole("button", { name: "Remove? Tasks are kept.: Engineering" });
    fireEvent.click(remove);
    expect(screen.queryByTestId("projects-band")).toBeNull();
    expect(baseProps.onDeleteSession).not.toHaveBeenCalled();
  });

  it("announces recently shown and archived section expansion states", () => {
    const recent = Array.from({ length: 6 }, (_, i) => ({
      ...SESSIONS[0],
      session_id: `recent-${i}`,
      title: `Recent item ${i}`,
      updated_at: `2026-07-0${i + 1}`,
    }));
    const archived = { ...SESSIONS[1], session_id: "archived-example", archived: true, title: "Old task" };
    renderSidebar({ ...baseProps, sessions: [...recent, archived] });
    const more = screen.getByRole("button", { name: /show 2 more/i });
    expect(more.className).toContain("ds-button-ghost");
    expect(more.getAttribute("aria-expanded")).toBe("false");
    expect(document.getElementById(more.getAttribute("aria-controls")!)).toBeTruthy();
    fireEvent.click(more);
    expect(more.getAttribute("aria-expanded")).toBe("true");
    expect(screen.getByText("Recent item 0")).toBeTruthy();
    const toggle = screen.getByRole("button", { name: "Archived (1)" });
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(toggle);
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    expect(document.getElementById(toggle.getAttribute("aria-controls")!)).toBeTruthy();
    expect(screen.getByText("Old task")).toBeTruthy();
  });
});
