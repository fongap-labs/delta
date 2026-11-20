import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { I18nProvider } from "@delta/i18n/I18nContext";
import { ScheduledView } from "./ScheduledView";
import {
  createAutomation, deleteAutomation, getAutomation, getAutomations,
  getDenialThreshold, markAutomationSeen, pickWorkspaceFolder,
} from "../api";

vi.mock("./AutomationQuickstart", () => ({ AutomationQuickstart: () => null }));
vi.mock("./IntegrationsView", () => ({
  PanelHead: ({ title }: { title: string }) => <h1>{title}</h1>,
}));
vi.mock("../api", () => ({
  createAutomation: vi.fn(), deleteAutomation: vi.fn(), getAutomation: vi.fn(),
  getAutomations: vi.fn(), getDenialThreshold: vi.fn(), markAutomationSeen: vi.fn(),
  setDenialThreshold: vi.fn(), announceAutomationsChanged: vi.fn(),
  pickWorkspaceFolder: vi.fn(), updateAutomation: vi.fn(),
}));

const TASK = {
  id: "task-1", title: "Daily report", instructions: "Summarize reports",
  schedule: "Every day at 9:00", schedule_raw: { kind: "cron", cron: "0 9 * * *" },
  workspace: "/workspace", enabled: true, next_run: null, run_count: 0,
  last_status: null, seen_runs_at: 0, always_allowed: [],
};

const onRunNow = vi.fn();
const onOpenRun = vi.fn();
function view(initialOpenId?: string) {
  return render(
    <I18nProvider locale="en-US">
      <ScheduledView onRunNow={onRunNow} onOpenRun={onOpenRun} initialOpenId={initialOpenId} />
    </I18nProvider>,
  );
}

beforeEach(() => {
  vi.mocked(getAutomations).mockResolvedValue([]);
  vi.mocked(getDenialThreshold).mockResolvedValue(3);
  vi.mocked(createAutomation).mockResolvedValue({ ok: false, error: "test failure" } as never);
  vi.mocked(pickWorkspaceFolder).mockResolvedValue("/workspace/reports");
  vi.mocked(getAutomation).mockResolvedValue({ task: TASK, runs: [] } as never);
  vi.mocked(markAutomationSeen).mockResolvedValue(undefined as never);
  vi.mocked(deleteAutomation).mockResolvedValue(undefined as never);
});
afterEach(() => { cleanup(); vi.clearAllMocks(); });

describe("ScheduledView shared UI controls", () => {
  it("validates manual scheduling before calling the creation API", async () => {
    view();
    const toggle = screen.getByRole("button", { name: /New automation/ });
    expect(toggle.className).toContain("ds-button-sm");
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(toggle);
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    expect(document.getElementById(toggle.getAttribute("aria-controls")!)).toBeTruthy();

    const submit = screen.getByRole("button", { name: "Create automation" }) as HTMLButtonElement;
    const title = screen.getByRole("textbox", { name: "Title" }) as HTMLInputElement;
    const instructions = screen.getByRole("textbox", { name: "Instructions" });
    expect(title.className).toContain("ds-field");
    expect(submit.className).toContain("ds-button-primary");
    expect(submit.disabled).toBe(true);
    fireEvent.change(title, { target: { value: "Daily report" } });
    fireEvent.change(instructions, { target: { value: "Summarize reports" } });
    expect(submit.disabled).toBe(false);
    fireEvent.click(submit);
    await waitFor(() => expect(createAutomation).toHaveBeenCalledWith({
      title: "Daily report", instructions: "Summarize reports", cron: "0 9 * * *",
    }));
    expect(await screen.findByRole("alert")).toBeTruthy();
  });

  it("keeps folder-triggered creation gated on a chosen workspace", async () => {
    view();
    fireEvent.click(screen.getByRole("button", { name: /New automation/ }));
    fireEvent.change(screen.getByRole("textbox", { name: "Title" }), { target: { value: "Watch PDFs" } });
    fireEvent.change(screen.getByRole("textbox", { name: "Instructions" }), { target: { value: "Process new PDFs" } });
    fireEvent.change(screen.getByTestId("trigger-kind"), { target: { value: "folder" } });
    const submit = screen.getByRole("button", { name: "Create automation" }) as HTMLButtonElement;
    expect(submit.disabled).toBe(true);
    fireEvent.change(screen.getByTestId("watch-pattern"), { target: { value: "*.pdf" } });
    expect(submit.disabled).toBe(true);
    fireEvent.click(screen.getByTestId("choose-watch-folder"));
    await waitFor(() => expect(submit.disabled).toBe(false));
    fireEvent.click(submit);
    await waitFor(() => expect(createAutomation).toHaveBeenCalledWith({
      title: "Watch PDFs", instructions: "Process new PDFs", workspace: "/workspace/reports",
      watch_folder: ".", watch_pattern: "*.pdf",
    }));
  });

  it("reuses task detail actions without changing run or edit semantics", async () => {
    view("task-1");
    const run = await screen.findByRole("button", { name: /Run now/ });
    expect(run.className).toContain("ds-button-primary");
    fireEvent.click(run);
    expect(onRunNow).toHaveBeenCalledWith("task-1", "Daily report");

    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    const title = screen.getByRole("textbox", { name: "Title" }) as HTMLInputElement;
    expect(title.className).toContain("ds-field");
    const save = screen.getByRole("button", { name: "Save" }) as HTMLButtonElement;
    expect(save.className).toContain("ds-button-primary");
    expect(save.disabled).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("textbox", { name: "Title" })).toBeNull();
    expect(screen.getByRole("button", { name: "Delete" }).className).toContain("ds-button-danger");
  });

  it("keeps list-item delete separate from opening the task", async () => {
    vi.mocked(getAutomations).mockResolvedValue([TASK] as never);
    view();
    const remove = await screen.findByRole("button", { name: "Delete Daily report" });
    expect(remove.getAttribute("type")).toBe("button");
    expect(remove.className).toContain("ds-icon-button");
    fireEvent.click(remove);
    await waitFor(() => expect(deleteAutomation).toHaveBeenCalledWith("task-1"));
    expect(getAutomation).not.toHaveBeenCalled();
  });
});
