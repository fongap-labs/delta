// SKILLS-SPEC §4.6 GUI — the transcript trust line: a load_skill tool call always renders
// as a human-readable "Used skill: X" step, whether model-invoked or forced via /skill.
import { describe, expect, it } from "vitest";
import { humanizeApprovalTitle, humanizeAsk, humanizeTool } from "./humanize";
import { scopeNote } from "./components/ApprovalCard";

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

describe("humanize editing and skill lookup tools", () => {
  it("says edit_document edited a named file, in the step, the approval and the ask", () => {
    expect(humanizeTool("edit_document", { path: "out/plan.docx" })).toMatchObject({ pre: "Edited ", obj: "plan.docx" });
    expect(humanizeApprovalTitle("edit_document", { path: "out/plan.docx" })).toMatchObject({ pre: "Edit ", obj: "plan.docx" });
    expect(humanizeAsk("edit_document", { path: "out/plan.docx" })).toMatchObject({ pre: "Wanted to edit ", obj: "plan.docx" });
  });

  it("shows a skill lookup and a skill read as plain steps", () => {
    expect(humanizeTool("find_skill", { query: "weekly report" })).toMatchObject({ pre: "Looked up your saved skills" });
    expect(humanizeTool("read_skill", { name: "weekly-report" })).toMatchObject({ pre: "Used skill: ", obj: "weekly-report" });
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

describe("humanize browser tools", () => {
  it("names the page element or text, so the approval is about something the user can see", () => {
    expect(humanizeApprovalTitle("browser_navigate", { url: "https://example.com/a" })).toMatchObject({
      pre: "Open in the Delta browser: ",
      obj: "https://example.com/a",
    });
    expect(humanizeApprovalTitle("browser_click", { ref: "e3", label: "Place order" })).toMatchObject({
      pre: "Click ",
      obj: "“Place order”",
    });
    expect(humanizeApprovalTitle("browser_type", { ref: "e1", label: "Email", text: "ada@example.com" })).toMatchObject({
      pre: "Type ",
      obj: "“ada@example.com”",
      post: " into “Email”",
      postVars: { label: "Email" },
    });
    expect(humanizeTool("browser_click", { label: "Next" })).toMatchObject({ pre: "Clicked ", obj: "“Next”" });
    expect(humanizeTool("browser_close", {})).toMatchObject({ pre: "Closed the Delta browser" });
  });

  it("explains in the scope note where browser actions happen", () => {
    const t = (key: string) => key;
    expect(scopeNote(t, "browser_click", { label: "Next" }, "computer").text).toBe("approval.scope.browser");
  });
});

describe("humanize statistics tools", () => {
  it("names the analysis rather than the tool", () => {
    expect(humanizeTool("statistics_ttest", { path: "a.csv" })).toMatchObject({ pre: "Ran a statistical analysis — ", obj: "ttest" });
    expect(humanizeTool("statistics_regression", {})).toMatchObject({ obj: "regression" });
    expect(humanizeTool("statistics_record", {})).toMatchObject({ obj: "record" });
    expect(humanizeApprovalTitle("statistics_record", { result_path: "out/t.json" })).toMatchObject({ obj: "t.json" });
    expect(humanizeTool("statistics_doe", {})).toMatchObject({ pre: "Laid out an experiment design" });
  });
});

describe("humanize library tools", () => {
  it("shows what was searched and says an index update reads files", () => {
    expect(humanizeTool("library_search", { query: "supplier audit" })).toMatchObject({
      pre: "Searched your library for ",
      obj: "“supplier audit”",
    });
    expect(humanizeTool("library_index", {})).toMatchObject({ pre: "Updated the library index" });
    expect(humanizeApprovalTitle("library_index", {}).pre).toContain("library index");
  });
});
