// Approval dialog (design 2:604): when it opens, what each button sends, and the keyboard rules.
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { JSX } from "react";
import {
  ApprovalModal,
  canRememberChoice,
  readDialogPref,
  usesApprovalModal,
  writeDialogPref,
} from "./ApprovalModal";
import { I18nProvider } from "@delta/i18n/I18nContext";
import type { Item } from "../types";

type Approval = Extract<Item, { kind: "approval" }>;

const approval = (over: Partial<Approval> = {}): Approval => ({
  kind: "approval",
  name: "write_file",
  args: { path: "notes.md", content: "hello" },
  reason: "",
  ...over,
});

const wrap = (el: JSX.Element) => <I18nProvider locale="en-US">{el}</I18nProvider>;

afterEach(() => {
  cleanup();
  localStorage.removeItem("delta-approval-dialog");
});

describe("which approvals get the dialog", () => {
  it("an outbound action opens it when the default egress level is Ask each time", () => {
    const outbound = approval({ name: "send_message", args: { text: "hi", target: "slack:#general" } });
    expect(usesApprovalModal(outbound, { egressLevel: "ask_each_time", importantOps: false })).toBe(true);
    for (const level of ["open", "approved_endpoints", "private_only", null] as const) {
      expect(usesApprovalModal(outbound, { egressLevel: level, importantOps: false })).toBe(false);
    }
  });

  it("a connector write always opens it", () => {
    const write = approval({ name: "crm_update", category: "connector", args: {} });
    expect(usesApprovalModal(write, { egressLevel: "open", importantOps: false })).toBe(true);
  });

  it("any other approval opens it only when the preference is on, never for scheduled-task grants", () => {
    const file = approval();
    expect(usesApprovalModal(file, { egressLevel: "open", importantOps: false })).toBe(false);
    expect(usesApprovalModal(file, { egressLevel: "open", importantOps: true })).toBe(true);
    const grants = approval({ name: "create_scheduled_task", args: { permissions: [] } });
    expect(usesApprovalModal(grants, { egressLevel: "open", importantOps: true })).toBe(false);
  });

  it("the session-wide remember choice follows the card's own rule", () => {
    expect(canRememberChoice(approval())).toBe(true);
    expect(canRememberChoice(approval({ name: "send_file", args: { path: "a.pdf" } }))).toBe(false);
    expect(canRememberChoice(approval({ name: "run_shell", args: { command: "ls" } }))).toBe(false);
    expect(canRememberChoice(approval({ category: "connector", name: "crm_update" }))).toBe(false);
  });
});

describe("ApprovalModal", () => {
  it("is a modal dialog, with focus moved in", () => {
    render(wrap(<ApprovalModal item={approval()} onApprove={vi.fn()} />));
    const dialog = screen.getByRole("dialog");
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(document.activeElement).toBe(dialog);
  });

  it("gives focus back to the element that opened it when it closes", () => {
    const opener = document.createElement("button");
    document.body.appendChild(opener);
    opener.focus();
    const { unmount } = render(wrap(<ApprovalModal item={approval()} onApprove={vi.fn()} />));
    expect(document.activeElement).not.toBe(opener);
    unmount();
    expect(document.activeElement).toBe(opener);
    opener.remove();
  });

  it("Escape refuses the action", () => {
    const onApprove = vi.fn();
    render(wrap(<ApprovalModal item={approval()} onApprove={onApprove} />));
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onApprove).toHaveBeenCalledWith("deny");
  });

  it("a click on the scrim refuses, and a click inside the dialog does not", () => {
    const onApprove = vi.fn();
    render(wrap(<ApprovalModal item={approval()} onApprove={onApprove} />));
    fireEvent.click(screen.getByRole("dialog"));
    expect(onApprove).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTestId("approval-modal-scrim"));
    expect(onApprove).toHaveBeenCalledWith("deny");
  });

  it("Tab cannot leave the dialog", () => {
    render(wrap(<ApprovalModal item={approval()} onApprove={vi.fn()} />));
    // The first focusable control is the Remember checkbox; Deny is the last.
    const first = screen.getByTestId("approval-remember").querySelector("input")!;
    const deny = screen.getByRole("button", { name: "Deny" });
    deny.focus();
    const forward = fireEvent.keyDown(deny, { key: "Tab" });
    expect(forward).toBe(false);
    expect(document.activeElement).toBe(first);
  });

  it("Remember my choice sends a session grant; otherwise the decision is once", () => {
    const onApprove = vi.fn();
    render(wrap(<ApprovalModal item={approval()} onApprove={onApprove} />));
    fireEvent.click(screen.getByTestId("approval-remember").querySelector("input")!);
    fireEvent.click(screen.getByRole("button", { name: "Allow once" }));
    expect(onApprove).toHaveBeenLastCalledWith("always_tool");
  });

  it("without the tick, Allow is a single approval", () => {
    const onApprove = vi.fn();
    render(wrap(<ApprovalModal item={approval()} onApprove={onApprove} />));
    fireEvent.click(screen.getByRole("button", { name: "Allow once" }));
    expect(onApprove).toHaveBeenLastCalledWith("once");
  });

  it("hides Remember for connector writes", () => {
    render(wrap(<ApprovalModal item={approval({ category: "connector", name: "crm_update", args: {} })} onApprove={vi.fn()} />));
    expect(screen.queryByTestId("approval-remember")).toBeNull();
  });

  it("an outbound action stays locked until the recipient and content are checked", () => {
    const onApprove = vi.fn();
    const outbound = approval({ name: "send_message", args: { text: "Standup moved", target: "slack:#general" } });
    render(wrap(<ApprovalModal item={outbound} onApprove={onApprove} />));
    const allow = screen.getByRole("button", { name: "Allow once" }) as HTMLButtonElement;
    expect(allow.disabled).toBe(true);
    fireEvent.click(screen.getByRole("checkbox", { name: /I have checked the recipient/ }));
    expect(allow.disabled).toBe(false);
    fireEvent.click(allow);
    expect(onApprove).toHaveBeenLastCalledWith("once");
  });

  it("Deny always works, even for an outbound action", () => {
    const onApprove = vi.fn();
    render(wrap(<ApprovalModal item={approval({ name: "send_message", args: { text: "x", target: "slack:#a" } })} onApprove={onApprove} />));
    fireEvent.click(screen.getByRole("button", { name: "Deny" }));
    expect(onApprove).toHaveBeenLastCalledWith("deny");
  });

  it("the preference is stored per device and reads back", () => {
    expect(readDialogPref()).toBe(false);
    writeDialogPref(true);
    expect(readDialogPref()).toBe(true);
    writeDialogPref(false);
    expect(readDialogPref()).toBe(false);
  });
});
