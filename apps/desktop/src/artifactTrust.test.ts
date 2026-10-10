import { describe, expect, it } from "vitest";
import { artifactTrustFromMessages, summarizeChecks } from "./artifactTrust";

const produced = (overrides: Record<string, unknown> = {}) => ({
  artifacts: [{ path: "reports/q3.docx", sha256: "ab12cd34ef56", run_id: "run-1" }],
  validation: {
    ok: true,
    checks: [
      { name: "artifact_count", ok: true, detail: "1 artifact" },
      { name: "all_artifacts_complete", ok: true, detail: "" },
    ],
  },
  ...overrides,
});

const messages = (output: unknown, asString = false) => [
  { role: "assistant", content: "", tool_calls: [{ id: "call-1", function: { name: "make_report" } }] },
  { role: "tool", tool_call_id: "call-1", content: asString ? JSON.stringify(output) : output },
];

describe("artifactTrustFromMessages", () => {
  it("reads the producer, checksum and validation verdict from the tool result", () => {
    const trust = artifactTrustFromMessages(messages(produced()), "reports/q3.docx");
    expect(trust).toMatchObject({ producedBy: "make_report", sha256: "ab12cd34ef56", isValidationOk: true });
    expect(summarizeChecks(trust!)).toEqual({ passed: 2, total: 2 });
  });

  it("accepts a JSON string result and Windows-style paths", () => {
    const trust = artifactTrustFromMessages(messages(produced(), true), "reports\\q3.docx");
    expect(trust?.sha256).toBe("ab12cd34ef56");
  });

  it("reports a failed check as not ok and counts it", () => {
    const failed = produced({
      validation: { ok: false, checks: [{ name: "required_paths", ok: false, detail: "missing" }, { name: "artifact_count", ok: true, detail: "" }] },
    });
    const trust = artifactTrustFromMessages(messages(failed), "reports/q3.docx")!;
    expect(trust.isValidationOk).toBe(false);
    expect(summarizeChecks(trust)).toEqual({ passed: 1, total: 2 });
  });

  it("returns nothing for an artifact no tool result registered, and never invents a verdict", () => {
    expect(artifactTrustFromMessages(messages(produced()), "other.txt")).toBeNull();
    expect(artifactTrustFromMessages([], "reports/q3.docx")).toBeNull();
    const noValidation = artifactTrustFromMessages(messages({ artifacts: [{ path: "a.txt" }] }), "a.txt")!;
    expect(noValidation.checks).toBeUndefined();
    expect(summarizeChecks(noValidation)).toBeNull();
  });

  it("uses the newest result when the same path was produced twice", () => {
    const first = { role: "tool", tool_call_id: "c1", content: produced({ artifacts: [{ path: "a.txt", sha256: "old" }] }) };
    const second = { role: "tool", tool_call_id: "c2", content: produced({ artifacts: [{ path: "a.txt", sha256: "new" }] }) };
    expect(artifactTrustFromMessages([first, second], "a.txt")?.sha256).toBe("new");
  });

  it("ignores malformed content", () => {
    expect(artifactTrustFromMessages([{ role: "tool", tool_call_id: "c", content: "not json" }], "a.txt")).toBeNull();
  });
});
