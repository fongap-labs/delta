// The `_display` sidecar on a persisted tool message (privacy-filter hidden
// counts) must surface on the replayed tool item — and only there; the
// agent-visible content string carries no trace.
import { describe, expect, it } from "vitest";
import type { Item } from "./types";
import { turnToolNames, itemsFromMessages, withUserIndexes } from "./itemsFromMessages";

describe("itemsFromMessages _display sidecar", () => {
  it("attaches hidden counts to the matching tool item", () => {
    const items = itemsFromMessages([
      { role: "user", content: "check my mail" },
      {
        role: "assistant",
        content: "",
        tool_calls: [
          { id: "t1", function: { name: "gmail_search_messages", arguments: "{}" } },
          { id: "t2", function: { name: "gmail_get_message", arguments: "{}" } },
        ],
      },
      {
        role: "tool",
        tool_call_id: "t1",
        content: '{"ok": true, "data": {"messages": []}}',
        _display: { hidden_by_filters: 2, connector: "gmail" },
      },
      { role: "tool", tool_call_id: "t2", content: '{"ok": true}' },
    ] as any);

    const tools = items.filter((i: any) => i.kind === "tool") as any[];
    expect(tools).toHaveLength(2);
    expect(tools[0].hidden).toBe(2);
    expect(tools[1].hidden).toBeUndefined();
    expect(tools[0].preview).not.toContain("hidden"); // content stays clean
  });
});

describe("itemsFromMessages timestamps", () => {
  it("carries the server ts through to user/assistant items; pre-stamp history gets none", () => {
    const items = itemsFromMessages([
      { role: "user", content: "hi", ts: 1752969720 },
      { role: "assistant", content: "hello", ts: 1752969724 },
      { role: "user", content: "old message" }, // saved before the server stamped ts
    ] as any);

    expect(items).toEqual([
      { kind: "user", text: "hi", ts: 1752969720, index: 0 },
      { kind: "assistant", text: "hello", ts: 1752969724, index: 1 },
      { kind: "user", text: "old message", index: 2 },
    ]);
  });
});

describe("itemsFromMessages notices", () => {
  it("replays persisted error/interrupted markers; only errors are retriable", () => {
    const items = itemsFromMessages([
      { role: "user", content: "hi" },
      { role: "assistant", content: "partial ans" },
      { role: "notice", kind: "interrupted", ts: 1752969720 },
      { role: "user", content: "again" },
      { role: "notice", kind: "error", text: "model down", ts: 1752969724 },
    ] as any);

    expect(items).toEqual([
      { kind: "user", text: "hi", index: 0 },
      { kind: "assistant", text: "partial ans", index: 1 },
      { kind: "notice", tone: "warn", text: "Interrupted." },
      { kind: "user", text: "again", index: 3 },
      { kind: "notice", tone: "warn", text: "Error: model down", retriable: true },
    ]);
  });
});

describe("itemsFromMessages model switch", () => {
  it("replays the persisted model_switch marker as an info notice", () => {
    const items = itemsFromMessages([
      { role: "user", content: "hi" },
      { role: "notice", kind: "model_switch", text: "Model switched to Kimi K2.6 · Moonshot" },
    ] as any);
    expect(items[1]).toEqual({
      kind: "notice",
      tone: "info",
      text: "Model switched to Kimi K2.6 · Moonshot",
      modelSwitchModel: undefined,
      modelSwitchImageWarning: false,
    });
  });
});

describe("itemsFromMessages compaction", () => {
  it("replays the persisted compacted marker as an info notice (the divider)", () => {
    const items = itemsFromMessages([
      { role: "user", content: "hi" },
      { role: "notice", kind: "compacted", text: "Context compacted — earlier turns were summarized" },
    ] as any);
    expect(items[1]).toEqual({
      kind: "notice",
      tone: "info",
      text: "Context compacted — earlier turns were summarized",
    });
  });
});

describe("itemsFromMessages reasoning", () => {
  it("attaches the reasoning sidecar to assistant items; thinking-only messages still render", () => {
    const items = itemsFromMessages([
      { role: "user", content: "hi" },
      { role: "assistant", content: "answer", reasoning: "let me think" },
      { role: "assistant", content: "", reasoning: "stopped mid-thought" },
    ] as any);
    expect(items[1]).toEqual({ kind: "assistant", text: "answer", reasoning: "let me think", index: 1 });
    expect(items[2]).toEqual({ kind: "assistant", text: "", reasoning: "stopped mid-thought", index: 2 });
  });
});

describe("itemsFromMessages answer index", () => {
  it("numbers an answer by its stored message so a branch can end there", () => {
    const items = itemsFromMessages([
      { role: "user", content: "q" },
      { role: "assistant", content: "", tool_calls: [{ id: "t1", function: { name: "web_search", arguments: "{}" } }] },
      { role: "tool", tool_call_id: "t1", content: "r" },
      { role: "assistant", content: "final" },
    ] as any);
    expect(items.filter((it) => it.kind === "assistant")).toEqual([{ kind: "assistant", text: "final", index: 3 }]);
  });
});

describe("withUserIndexes", () => {
  it("gives an optimistic user bubble the index of its stored twin, newest match first", () => {
    const persisted = itemsFromMessages([
      { role: "user", content: "继续" },
      { role: "assistant", content: "好的" },
      { role: "user", content: "继续" },
    ] as any);
    const current: any[] = [
      { kind: "user", text: "继续", index: 0 },
      { kind: "assistant", text: "好的" },
      { kind: "user", text: "继续", ts: 5 },
    ];
    const out = withUserIndexes(current, persisted);
    expect(out[2]).toMatchObject({ kind: "user", index: 2, ts: 5 });
    expect(out[0]).toBe(current[0]);
  });

  it("leaves a bubble without a stored twin alone", () => {
    const current: any[] = [{ kind: "user", text: "not stored yet" }];
    expect(withUserIndexes(current, [])[0]).toEqual({ kind: "user", text: "not stored yet" });
  });
});

describe("itemsFromMessages retries", () => {
  it("does not replay an automatic retry marker as an error", () => {
    const items = itemsFromMessages([
      { role: "user", content: "hi" },
      { role: "notice", kind: "retrying", text: "Retrying (attempt 1/2)" },
    ] as any);
    expect(items).toHaveLength(1);
  });
});

describe("turnToolNames", () => {
  it("counts only the tool calls since the last user message", () => {
    const items: Item[] = [
      { kind: "user", text: "first" },
      { kind: "tool", id: "a", name: "web_search", args: {}, status: "ok" },
      { kind: "tool", id: "b", name: "web_fetch", args: {}, status: "ok" },
      { kind: "assistant", text: "answer", ts: 1 },
      { kind: "user", text: "second" },
      { kind: "tool", id: "c", name: "web_search", args: {}, status: "…" },
    ];
    expect(turnToolNames(items)).toEqual(["web_search"]);
  });

  it("is empty for a request that has not called a tool yet", () => {
    expect(turnToolNames([{ kind: "user", text: "hi" }])).toEqual([]);
  });
});
