// SKILLS-SPEC §4.6 GUI — the transcript trust line: a load_skill tool call always renders
// as a human-readable "Used skill: X" step, whether model-invoked or forced via /skill.
import { describe, expect, it } from "vitest";
import { humanizeApprovalTitle, humanizeAsk, humanizeTool } from "./humanize";

describe("humanizeTool(load_skill)", () => {
  it("renders the Used-skill line with the skill name", () => {
    const line = humanizeTool("load_skill", { name: "incident-summary" });
    expect(line.pre).toBe("Used skill: ");
    expect(line.obj).toBe("incident-summary");
  });

  it("stays safe on null/missing args", () => {
    expect(humanizeTool("load_skill", null).obj).toBe("");
    expect(humanizeTool("load_skill", {}).obj).toBe("");
  });
});

describe("humanize document and web tools", () => {
  it("names the document, the page and the query instead of the tool", () => {
    expect(humanizeTool("read_document", { path: "reports/q3.xlsx" })).toMatchObject({ pre: "Read ", obj: "q3.xlsx" });
    expect(humanizeTool("create_document", { path: "out/plan.docx" })).toMatchObject({ pre: "Created ", obj: "plan.docx" });
    expect(humanizeApprovalTitle("web_fetch", { url: "https://example.com/a?b=1" })).toMatchObject({
      pre: "Read the web page ",
      obj: "example.com",
    });
    expect(humanizeApprovalTitle("web_search", { query: "rust" })).toMatchObject({ obj: "“rust”" });
    expect(humanizeApprovalTitle("create_document", { path: "plan.docx" })).toMatchObject({ pre: "Create ", obj: "plan.docx" });
    expect(humanizeAsk("web_fetch", { url: "https://example.com/" })).toMatchObject({ pre: "Wanted to read the web page " });
  });
});

describe("humanize computer tools", () => {
  it("says what the screen, clipboard and open tools do in plain words", () => {
    expect(humanizeApprovalTitle("screenshot", {})).toMatchObject({ pre: "Take a picture of your screen" });
    expect(humanizeApprovalTitle("clipboard_read", {})).toMatchObject({ pre: "Read what you copied to the clipboard" });
    expect(humanizeApprovalTitle("open_file", { path: "out/q3.xlsx" })).toMatchObject({ pre: "Open ", obj: "q3.xlsx" });
    expect(humanizeApprovalTitle("open_file", { path: "out/q3.xlsx", reveal: true })).toMatchObject({ pre: "Show in the file manager: " });
    expect(humanizeApprovalTitle("open_url", { url: "https://example.com/a" })).toMatchObject({ obj: "https://example.com/a" });
    expect(humanizeTool("screenshot", {})).toMatchObject({ pre: "Looked at your screen" });
    expect(humanizeTool("open_url", { url: "https://example.com/a" })).toMatchObject({ obj: "example.com" });
  });
});
