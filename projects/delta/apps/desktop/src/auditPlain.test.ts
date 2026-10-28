import { describe, expect, it } from "vitest";
import { auditArgs, auditLine, auditState, auditWhen } from "./auditPlain";

describe("auditArgs", () => {
  it("reads arguments stored as JSON text instead of splitting them into characters", () => {
    expect(auditArgs('{"query":"今天是几月几号"}')).toEqual({ query: "今天是几月几号" });
  });

  it("passes an object through and treats anything unreadable as empty", () => {
    expect(auditArgs({ url: "https://example.com" })).toEqual({ url: "https://example.com" });
    expect(auditArgs("not json")).toEqual({});
    expect(auditArgs(["a"])).toEqual({});
    expect(auditArgs(undefined)).toEqual({});
  });
});

describe("auditLine", () => {
  it("describes a web search by its query", () => {
    expect(auditLine({ tool: "web_search", args: '{"query":"rust release"}' })).toEqual({
      key: "activity.plain.search",
      params: { query: "rust release" },
    });
  });

  it("describes a page open by its address and clips very long values", () => {
    const long = "x".repeat(200);
    const line = auditLine({ tool: "web_fetch", args: { url: long } });
    expect(line.key).toBe("activity.plain.fetch");
    expect(line.params.url.length).toBe(81);
  });

  it("falls back to the tool name for anything else", () => {
    expect(auditLine({ tool: "email_send", args: "" })).toEqual({
      key: "activity.plain.tool",
      params: { tool: "email_send" },
    });
  });
});

describe("auditState", () => {
  it("says a request is waiting for the user, and what the user chose", () => {
    expect(auditState({ stage: "approval_required", status: "pending" })).toBe("waiting");
    expect(auditState({ stage: "approval_resolved", status: "approved" })).toBe("approved");
    expect(auditState({ stage: "approval_resolved", status: "denied" })).toBe("denied");
  });

  it("reports completion and failure", () => {
    expect(auditState({ stage: "finished", status: "completed" })).toBe("done");
    expect(auditState({ stage: "finished", status: "failed" })).toBe("failed");
    expect(auditState({ stage: "", status: "" })).toBe("other");
  });
});

describe("auditWhen", () => {
  it("shows the time alone for today and the date for an earlier day", () => {
    const now = new Date(2026, 9, 15, 15, 0, 0);
    const today = new Date(2026, 9, 15, 12, 45, 12).toISOString();
    const earlier = new Date(2026, 9, 14, 9, 5, 0).toISOString();
    expect(auditWhen(today, now)).toMatch(/12:45/);
    expect(auditWhen(earlier, now)).toMatch(/14|10/);
    expect(auditWhen(earlier, now)).not.toBe(auditWhen(today, now));
  });

  it("accepts nanosecond fractions and returns unreadable text unchanged", () => {
    const now = new Date(2026, 9, 15, 15, 0, 0);
    expect(auditWhen("2026-10-15T12:45:12.5912612Z", now)).not.toBe("2026-10-15T12:45:12.5912612Z");
    expect(auditWhen("whenever", now)).toBe("whenever");
  });
});
