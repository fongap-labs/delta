import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { deliveredSinceLastUser, filesDeliveredSinceLastUser, Transcript } from "./Transcript";
import { SessionIntro } from "./SessionIntro";
import { I18nProvider } from "@delta/i18n/I18nContext";
import type { Item } from "../types";

afterEach(cleanup);

const wrap = (ui: React.ReactElement) => <I18nProvider locale="en-US">{ui}</I18nProvider>;
const write = (path: string, status = "ok"): Item => ({ kind: "tool", id: path, name: "write_file", args: { path }, status });

describe("delivery card", () => {
  it("lists each file the last request wrote once, skipping failed and pending writes", () => {
    const items: Item[] = [
      { kind: "user", text: "old" },
      write("/w/old.md"),
      { kind: "user", text: "make a report" },
      write("/w/report.docx"),
      write("/w/report.docx"),
      write("/w/data.csv"),
      write("/w/bad.txt", "error"),
      write("/w/pending.txt", "…"),
      { kind: "assistant", text: "Done." },
    ];
    expect(filesDeliveredSinceLastUser(items)).toEqual(["/w/report.docx", "/w/data.csv"]);
  });

  it("shows when the request is finished and opens the preview on click", () => {
    const opened = vi.fn();
    window.addEventListener("delta-open-artifact", (e) => opened((e as CustomEvent).detail.path));
    const items: Item[] = [{ kind: "user", text: "go" }, write("/w/report.docx"), { kind: "assistant", text: "Done." }];
    const { rerender } = render(wrap(<Transcript items={items} running={true} onApprove={() => {}} />));
    expect(screen.queryByTestId("delivery-card")).toBeNull();
    rerender(wrap(<Transcript items={items} running={false} onApprove={() => {}} />));
    expect(screen.getByTestId("delivery-card").textContent).toContain("report.docx");
    fireEvent.click(screen.getByTestId("delivery-open"));
    expect(opened).toHaveBeenCalledWith("/w/report.docx");
  });
});

describe("steer receipt and failure actions", () => {
  it("marks a message sent during a run as added to the current task", () => {
    const items: Item[] = [{ kind: "user", text: "use the short path", steer: true }];
    render(wrap(<Transcript items={items} running={true} onApprove={() => {}} />));
    expect(screen.getByTestId("steer-receipt").textContent).toBe("Added to the current task");
  });

  it("offers Switch model next to Retry on a provider failure", () => {
    const onSwitchModel = vi.fn();
    const items: Item[] = [{ kind: "notice", tone: "warn", text: "Error: HTTP 503", retriable: true }];
    render(wrap(<Transcript items={items} running={false} onApprove={() => {}} onRetry={() => {}} onSwitchModel={onSwitchModel} />));
    fireEvent.click(screen.getByTestId("notice-switch-model"));
    expect(onSwitchModel).toHaveBeenCalledOnce();
  });
});

describe("starting points", () => {
  it("fills the composer with the chosen example and nothing else", () => {
    const onPick = vi.fn();
    render(wrap(<SessionIntro onPick={onPick} />));
    const buttons = screen.getAllByTestId("intro-example");
    expect(buttons).toHaveLength(4);
    fireEvent.click(buttons[2]);
    expect(onPick).toHaveBeenCalledWith(buttons[2].textContent);
  });
});

describe("save as skill", () => {
  const tool = (name: string): Item => ({ kind: "tool", id: name + Math.random(), name, args: {}, status: "ok" });
  it("is offered after a request with at least three steps, once the run has ended", () => {
    const onSave = vi.fn();
    const items: Item[] = [{ kind: "user", text: "go" }, tool("read_file"), tool("web_search"), tool("web_fetch"), { kind: "assistant", text: "Done." }];
    const { rerender } = render(wrap(<Transcript items={items} running={true} onApprove={() => {}} onSaveAsSkill={onSave} />));
    expect(screen.queryByTestId("save-as-skill")).toBeNull();
    rerender(wrap(<Transcript items={items} running={false} onApprove={() => {}} onSaveAsSkill={onSave} />));
    fireEvent.click(screen.getByTestId("save-as-skill"));
    expect(onSave).toHaveBeenCalledOnce();
  });

  it("is not offered for a quick request", () => {
    const items: Item[] = [{ kind: "user", text: "hi" }, tool("read_file"), { kind: "assistant", text: "Hello." }];
    render(wrap(<Transcript items={items} running={false} onApprove={() => {}} onSaveAsSkill={() => {}} />));
    expect(screen.queryByTestId("save-as-skill")).toBeNull();
  });
});

describe("restoring a replaced file", () => {
  const replaced = (): Item[] => [
    { kind: "user", text: "update the report" },
    {
      kind: "tool", id: "t1", name: "write_file", args: { path: "reports/q3.md" }, status: "ok",
      preview: JSON.stringify({ ok: true, artifacts: [{ path: "reports/q3.md", previous_version: ".delta/versions/run-1/reports/q3.md" }] }),
    },
    { kind: "assistant", text: "Updated." },
  ];

  it("reads the kept version from the stored tool result, and ignores a cut-off preview", () => {
    expect(deliveredSinceLastUser(replaced())).toEqual([
      { path: "reports/q3.md", previous: ".delta/versions/run-1/reports/q3.md" },
    ]);
    const cut = replaced();
    (cut[1] as any).preview = '{"ok":true,"artifacts":[{"path":"reports/q3';
    expect(deliveredSinceLastUser(cut)).toEqual([{ path: "reports/q3.md" }]);
  });

  it("offers the restore, reports success and shows a failure's reason", async () => {
    const restore = vi.fn().mockResolvedValueOnce("that earlier version is no longer available").mockResolvedValueOnce(null);
    render(wrap(<Transcript items={replaced()} running={false} onApprove={() => {}} onRestoreVersion={restore} />));
    fireEvent.click(screen.getByTestId("restore-version"));
    expect((await screen.findByTestId("restore-error")).textContent).toContain("no longer available");
    fireEvent.click(screen.getByTestId("restore-version"));
    await screen.findByTestId("restore-done");
    expect(restore).toHaveBeenCalledWith(".delta/versions/run-1/reports/q3.md", "reports/q3.md");
  });
});
