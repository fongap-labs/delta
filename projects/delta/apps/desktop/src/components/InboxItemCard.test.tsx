import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { I18nProvider } from "@delta/i18n/I18nContext";
import type { InboxItem } from "../api";
import { InboxItemCard } from "./InboxItemCard";

afterEach(cleanup);

function item(kind: InboxItem["kind"], extra: Partial<InboxItem> = {}): InboxItem {
  return {
    id: "inbox-42", session_id: "session-1", kind, title: "Choose deployment target",
    body: "", state: "pending", resolution: null, inbox: "default",
    created_at: "", resolved_at: null, ...extra,
  };
}
const view = (it: InboxItem, onResolve = vi.fn()) =>
  render(
    <I18nProvider locale="en-US">
      <InboxItemCard item={it} onResolve={onResolve} />
    </I18nProvider>,
  );

describe("InboxItemCard shared action controls", () => {
  it("keeps a multiple-choice question opt-in with pressed choice state", () => {
    const onResolve = vi.fn();
    view(item("question", { options: ["staging", "production"], multi: true, allow_text: false }), onResolve);
    const staging = screen.getByRole("button", { name: "staging" });
    expect(staging.className).toContain("ds-button");
    expect(staging.getAttribute("type")).toBe("button");
    expect(staging.getAttribute("aria-pressed")).toBe("false");
    const send = screen.getByRole("button", { name: "Send" }) as HTMLButtonElement;
    expect(send.disabled).toBe(true);
    fireEvent.click(staging);
    expect(staging.getAttribute("aria-pressed")).toBe("true");
    expect(onResolve).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Send (1)" }));
    expect(onResolve).toHaveBeenCalledTimes(1);
    expect(onResolve).toHaveBeenCalledWith("inbox-42", "staging");
  });

  it("accepts a typed question reply only after Send or Enter", () => {
    const onResolve = vi.fn();
    view(item("question", { allow_text: true }), onResolve);
    const input = screen.getByRole("textbox", { name: "Your answer…" });
    expect(input.className).toContain("ds-field");
    const send = screen.getByRole("button", { name: "Send" }) as HTMLButtonElement;
    expect(send.disabled).toBe(true);
    fireEvent.change(input, { target: { value: "staging" } });
    expect(onResolve).not.toHaveBeenCalled();
    fireEvent.click(send);
    expect(onResolve).toHaveBeenCalledWith("inbox-42", "staging");
  });

  it("keeps outbound approval and standing grant locked until confirmation, but Deny available", () => {
    const onResolve = vi.fn();
    view(item("approval", {
      title: "Send message?",
      data: {
        tool: "send_message", arguments: { target: "slack:T1/C1", text: "Report" },
        task_id: "task-1", task_title: "Daily report", standing_target: "slack:T1/C1",
      },
    }), onResolve);
    const allow = screen.getByRole("button", { name: /Allow once/ }) as HTMLButtonElement;
    const every = screen.getByRole("button", { name: "Allow every time" }) as HTMLButtonElement;
    expect(allow.className).toContain("ds-button");
    expect(allow.disabled).toBe(true);
    expect(every.disabled).toBe(true);
    const deny = screen.getByRole("button", { name: "Deny" }) as HTMLButtonElement;
    expect(deny.disabled).toBe(false);
    fireEvent.click(allow);
    expect(onResolve).not.toHaveBeenCalled();
    fireEvent.click(screen.getByLabelText("I have checked the recipient and the content"));
    expect(allow.disabled).toBe(false);
    expect(every.disabled).toBe(false);
    fireEvent.click(allow);
    expect(onResolve).toHaveBeenCalledWith("inbox-42", "allow");
  });

  it("preserves exact directory grant scope and explicit denial", () => {
    const onResolve = vi.fn();
    view(item("directory", { data: { path: "/work/docs", writable: false } }), onResolve);
    const grant = screen.getByRole("button", { name: /Grant/ });
    expect(grant.className).toContain("ds-button-primary");
    fireEvent.click(grant);
    expect(onResolve).toHaveBeenCalledWith("inbox-42", JSON.stringify({
      granted: true, path: "/work/docs", writable: false,
    }));
    fireEvent.click(screen.getByRole("button", { name: "Deny" }));
    expect(onResolve).toHaveBeenCalledWith("inbox-42", JSON.stringify({ granted: false }));
  });

  it("single-select answers immediately without escalating to task-wide permission", () => {
    const onResolve = vi.fn();
    view(item("question", { options: ["staging", "production"], allow_text: false }), onResolve);
    const staging = screen.getByRole("button", { name: "staging" });
    expect(staging.getAttribute("aria-pressed")).toBeNull();
    fireEvent.click(staging);
    expect(onResolve).toHaveBeenCalledWith("inbox-42", "staging");
    expect(onResolve).toHaveBeenCalledTimes(1);
  });
});
