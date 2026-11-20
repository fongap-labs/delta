import { describe, expect, it } from "vitest";
import type { Item } from "./types";
import { finishToolCall, formatToolDuration, markToolStart, readEventMs, toolDurationMs } from "./toolTiming";

const base = 1_800_000_000_000;
const rows = (): Item[] => [
  { kind: "tool", id: "a", name: "read_file", args: {}, status: "…" },
  { kind: "tool", id: "b", name: "read_file", args: {}, status: "…" },
];

describe("runtime tool event timing", () => {
  it("pairs timestamps by call ID, not by duplicate tool names", () => {
    const started = markToolStart(rows(), "a", base);
    const result = finishToolCall(started, { id: "a", name: "read_file", status: "ok", atMs: base + 1500 });
    expect(result[0]).toMatchObject({ id: "a", startedAtMs: base, finishedAtMs: base + 1500, status: "ok" });
    expect(result[1]).toMatchObject({ id: "b", status: "…" });
    expect(toolDurationMs(result[0] as Extract<Item, { kind: "tool" }>)).toBe(1500);
    expect(formatToolDuration(1500)).toBe("1.5 s");
    expect(result[1]).toBe(started[1]);
  });

  it("rejects invalid runtime timestamps without mutating pending calls", () => {
    for (const value of [0, -1, 1.5, Number.POSITIVE_INFINITY, Number.NEGATIVE_INFINITY, Number.MAX_SAFE_INTEGER + 1, null, undefined]) {
      expect(readEventMs(value)).toBeUndefined();
      const before = rows();
      expect(markToolStart(before, "a", value)).toBe(before);
    }
    expect(markToolStart(rows(), "a", base)[0]).toMatchObject({ startedAtMs: base });
    const started = markToolStart(rows(), "a", base);
    const finished = finishToolCall(started, {
      id: "a",
      name: "read_file",
      status: "ok",
      atMs: Number.POSITIVE_INFINITY,
    });
    expect(toolDurationMs(finished[0] as Extract<Item, { kind: "tool" }>)).toBeNull();
  });

  it("withholds duration when timestamps are missing or reversed", () => {
    expect(readEventMs(Number.NaN)).toBeUndefined();
    expect(readEventMs("1800000000000")).toBeUndefined();
    expect(markToolStart(rows(), "a", undefined)).toEqual(rows());
    const missing = finishToolCall(rows(), { id: "a", name: "read_file", status: "ok", atMs: base + 10 });
    expect(toolDurationMs(missing[0] as Extract<Item, { kind: "tool" }>)).toBeNull();
    const backwards = finishToolCall(markToolStart(rows(), "a", base + 10), { id: "a", name: "read_file", status: "ok", atMs: base });
    expect(toolDurationMs(backwards[0] as Extract<Item, { kind: "tool" }>)).toBeNull();
  });

  it("keeps the original start time when runtime start events are replayed", () => {
    const original = markToolStart(rows(), "a", base);
    const repeated = markToolStart(original, "a", base + 900);
    expect(repeated).toBe(original);
    const finished = finishToolCall(repeated, {
      id: "a",
      name: "read_file",
      status: "ok",
      atMs: base + 1000,
    });
    expect(toolDurationMs(finished[0] as Extract<Item, { kind: "tool" }>)).toBe(1000);
  });

  it("does not match a different ID or revive an interrupted call", () => {
    const input = rows();
    expect(markToolStart(input, "missing", base)).toBe(input);
    expect(finishToolCall(input, { id: "other", name: "read_file", status: "ok" })).toBe(input);
    const interrupted: Item[] = [{ kind: "tool", id: "a", name: "read_file", args: {}, status: "interrupted" }];
    expect(finishToolCall(interrupted, { id: "a", name: "read_file", status: "ok", atMs: base + 3 })).toBe(interrupted);
  });

  it("ignores replayed completion events without changing the first result", () => {
    const started = markToolStart(rows(), "a", base);
    const first = finishToolCall(started, {
      id: "a",
      name: "read_file",
      status: "ok",
      preview: "original",
      atMs: base + 800,
    });
    const replay = finishToolCall(first, {
      id: "a",
      name: "read_file",
      status: "error",
      preview: "replayed",
      atMs: base + 1200,
    });
    expect(replay).toBe(first);
    expect(replay[0]).toMatchObject({
      status: "ok",
      preview: "original",
      finishedAtMs: base + 800,
    });
    expect(toolDurationMs(replay[0] as Extract<Item, { kind: "tool" }>)).toBe(800);
  });

  it("preserves legacy no-ID updates without pretending to know timing", () => {
    const result = finishToolCall(rows(), { name: "read_file", status: "error", preview: "denied" });
    expect(result[1]).toMatchObject({ status: "error", preview: "denied" });
    expect(result[0]).toMatchObject({ status: "…" });
    expect(toolDurationMs(result[1] as Extract<Item, { kind: "tool" }>)).toBeNull();
    expect(formatToolDuration(42)).toBe("42 ms");
    expect(formatToolDuration(62_000)).toBe("1m 2s");
  });
});
