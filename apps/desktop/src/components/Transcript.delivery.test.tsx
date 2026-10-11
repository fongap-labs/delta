import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { filesDeliveredSinceLastUser, Transcript } from "./Transcript";
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
