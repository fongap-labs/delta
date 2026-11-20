import { describe, expect, it } from "vitest";
import type { Item } from "./types";
import { summarizeActivity } from "./activityMetrics";

describe("summarizeActivity", () => {
  it("counts actual recorded tool rows and states without inventing results", () => {
    const items: Item[] = [
      { kind: "user", text: "review" },
      { kind: "tool", id: "ok", name: "read_file", args: {}, status: "ok", startedAtMs: 1_800_000_000_000, finishedAtMs: 1_800_000_001_200 },
      { kind: "tool", id: "failed", name: "search", args: {}, status: "error", startedAtMs: 1_800_000_000_100, finishedAtMs: 1_800_000_002_200 },
      { kind: "tool", id: "live", name: "fetch", args: {}, status: "…" },
      { kind: "tool", id: "stop", name: "fetch", args: {}, status: "interrupted" },
      { kind: "tool", id: "missing", name: "read_file", args: {}, status: "unknown" },
      { kind: "assistant", text: "done" },
    ];
    expect(summarizeActivity(items)).toEqual({
      total: 5, completed: 1, failed: 1, running: 1, interrupted: 1, unknown: 1,
      timed: 2, longestMs: 2100,
    });
  });

  it("does not mistake an older or incomplete call for a measured duration", () => {
    const items: Item[] = [
      { kind: "tool", id: "old", name: "search", args: {}, status: "ok" },
      { kind: "tool", id: "unpaired", name: "read_file", args: {}, status: "ok", startedAtMs: 1_800_000_000_000 },
      { kind: "tool", id: "reverse", name: "fetch", args: {}, status: "error", startedAtMs: 1_800_000_001_000, finishedAtMs: 1_800_000_000_500 },
      { kind: "tool", id: "interrupted", name: "fetch", args: {}, status: "interrupted", startedAtMs: 1_800_000_000_000, finishedAtMs: 1_800_000_002_000 },
    ];
    const result = summarizeActivity(items);
    expect(result.total).toBe(4);
    expect(result.timed).toBe(0);
    expect(result.longestMs).toBeNull();
  });

  it("returns zero counts with no estimated duration for a tool-free session", () => {
    expect(summarizeActivity([{ kind: "user", text: "hello" }])).toEqual({
      total: 0, completed: 0, failed: 0, running: 0, interrupted: 0, unknown: 0,
      timed: 0, longestMs: null,
    });
  });
});
