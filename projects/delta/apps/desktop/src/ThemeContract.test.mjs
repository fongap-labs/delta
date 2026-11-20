import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

// The Vitest CSS stub is not the authored stylesheet; inspect source files directly.
const theme = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
const controls = readFileSync(new URL("./ui.css", import.meta.url), "utf8");

// Source-level theme contracts, not a substitute for screenshot or contrast audits.
function rule(source, selector) {
  const start = source.indexOf(selector + " {");
  if (start < 0) throw new Error("Missing CSS rule: " + selector);
  const open = source.indexOf("{", start);
  const close = source.indexOf("}", open + 1);
  if (close < 0) throw new Error("Unclosed CSS rule: " + selector);
  return source.slice(open + 1, close);
}

function token(block, name) {
  const match = block.match(new RegExp("--" + name + "\\s*:\\s*([^;]+)"));
  if (!match) throw new Error("Missing theme token: " + name);
  return match[1].trim();
}

describe("desktop visual theme contract", () => {
  it("keeps the established light and dark primary accent colors", () => {
    const light = rule(theme, ":root");
    const dark = rule(theme, 'html[data-theme="dark"]');
    expect(token(light, "teal-600")).toBe("#286f78");
    expect(token(light, "brand")).toBe("#286f78");
    expect(token(light, "primary")).toBe("var(--teal-600)");
    expect(token(dark, "primary")).toBe("#60b4bf");
    expect(token(light, "on-primary")).toBe("#ffffff");
    expect(token(dark, "on-primary")).toBe("#0e1a1c");
  });

  it("keeps interactive colors semantic and owned by the main theme", () => {
    expect(controls.match(/#[0-9a-f]{3,8}\b/gi)).toBeNull();
    expect(rule(controls, ".ds-button-primary")).toContain("var(--primary)");
    expect(rule(controls, ".ds-button-primary")).toContain("var(--on-primary)");
    expect(rule(controls, ".ds-button-primary:hover:not(:disabled)")).toContain("var(--primary-hover)");
    expect(rule(controls, ".ds-button-primary:active:not(:disabled)")).toContain("var(--primary-pressed)");
    expect(rule(controls, ".ds-button-danger")).toContain("var(--danger)");
  });

  it("preserves visible focus for keyboard actions and editable fields", () => {
    const buttonFocus = rule(controls, ".ds-button:focus-visible");
    expect(buttonFocus).toContain("2px solid var(--focus-ring)");
    expect(buttonFocus).toContain("outline-offset: 2px");
    const inputFocus = rule(controls, ".ds-field:focus-visible");
    expect(inputFocus).toContain("var(--focus-ring)");
    expect(inputFocus).toContain("var(--ui-ring)");
    expect(rule(controls, ".ds-icon-button:focus-visible,\n.ds-disclosure-trigger:focus-visible"))
      .toContain("var(--focus-ring)");
  });

  it("retains reduced-motion support for shared interactive surfaces", () => {
    expect(controls).toContain("@media (prefers-reduced-motion: reduce)");
    expect(controls).toContain(".ds-button, .ds-field, .ds-nav-button");
    expect(controls).toContain("transition: none");
  });
});
