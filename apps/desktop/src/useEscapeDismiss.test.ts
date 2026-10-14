import { describe, expect, it } from "vitest";
import { shouldDismissEscape } from "./useEscapeDismiss";

const press = (over: Record<string, unknown> = {}) =>
  ({ key: "Escape", defaultPrevented: false, isComposing: false, keyCode: 27, target: null, ...over }) as Parameters<typeof shouldDismissEscape>[0];

describe("shouldDismissEscape", () => {
  it("accepts a plain Escape", () => {
    expect(shouldDismissEscape(press())).toBe(true);
  });
  it("ignores other keys", () => {
    expect(shouldDismissEscape(press({ key: "Enter" }))).toBe(false);
  });
  it("leaves Escape to an input method that is composing text", () => {
    expect(shouldDismissEscape(press({ isComposing: true }))).toBe(false);
    expect(shouldDismissEscape(press({ keyCode: 229 }))).toBe(false);
  });
  it("leaves Escape to a handler that already used it", () => {
    expect(shouldDismissEscape(press({ defaultPrevented: true }))).toBe(false);
  });
  it("can leave Escape to text fields", () => {
    const field = { tagName: "TEXTAREA", isContentEditable: false };
    expect(shouldDismissEscape(press({ target: field }), { shouldIgnoreFields: true })).toBe(false);
    expect(shouldDismissEscape(press({ target: field }))).toBe(true);
    const editable = { tagName: "DIV", isContentEditable: true };
    expect(shouldDismissEscape(press({ target: editable }), { shouldIgnoreFields: true })).toBe(false);
    expect(shouldDismissEscape(press({ target: { tagName: "BUTTON", isContentEditable: false } }), { shouldIgnoreFields: true })).toBe(true);
  });
});
