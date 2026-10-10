import { describe, expect, it } from "vitest";
import { shouldDismissOnEscape } from "./useEscapeDismiss";

const press = (over: Record<string, unknown> = {}) =>
  ({ key: "Escape", defaultPrevented: false, isComposing: false, keyCode: 27, target: null, ...over }) as Parameters<typeof shouldDismissOnEscape>[0];

describe("shouldDismissOnEscape", () => {
  it("accepts a plain Escape", () => {
    expect(shouldDismissOnEscape(press())).toBe(true);
  });
  it("ignores other keys", () => {
    expect(shouldDismissOnEscape(press({ key: "Enter" }))).toBe(false);
  });
  it("leaves Escape to an input method that is composing text", () => {
    expect(shouldDismissOnEscape(press({ isComposing: true }))).toBe(false);
    expect(shouldDismissOnEscape(press({ keyCode: 229 }))).toBe(false);
  });
  it("leaves Escape to a handler that already used it", () => {
    expect(shouldDismissOnEscape(press({ defaultPrevented: true }))).toBe(false);
  });
  it("can leave Escape to text fields", () => {
    const field = { tagName: "TEXTAREA", isContentEditable: false };
    expect(shouldDismissOnEscape(press({ target: field }), { shouldIgnoreFields: true })).toBe(false);
    expect(shouldDismissOnEscape(press({ target: field }))).toBe(true);
    const editable = { tagName: "DIV", isContentEditable: true };
    expect(shouldDismissOnEscape(press({ target: editable }), { shouldIgnoreFields: true })).toBe(false);
    expect(shouldDismissOnEscape(press({ target: { tagName: "BUTTON", isContentEditable: false } }), { shouldIgnoreFields: true })).toBe(true);
  });
});
