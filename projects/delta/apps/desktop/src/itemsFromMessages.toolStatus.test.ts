// A replayed tool row takes its outcome from the stored tool result: a result with an `error` reads
// as a failed step, so it does not turn green again when the conversation is reopened.
import { describe, expect, it } from "vitest";
import { interruptActiveTools, itemsFromMessages } from "./itemsFromMessages";

describe("itemsFromMessages tool outcome", () => {
  it("replays a tool result that carries an error as a failed step", () => {
    const items = itemsFromMessages([
      { role: "user", content: "fetch both" },
      {
        role: "assistant",
        content: "",
        tool_calls: [
          { id: "c1", function: { name: "web_fetch", arguments: '{"url":"https://a.test"}' } },
          { id: "c2", function: { name: "web_fetch", arguments: '{"url":"https://b.test"}' } },
        ],
      },
      { role: "tool", tool_call_id: "c1", content: "page text", error: null },
      { role: "tool", tool_call_id: "c2", content: null, error: "timed out" },
    ] as any);
    const statuses = items.flatMap((it) => (it.kind === "tool" ? [it.status] : []));
    expect(statuses).toEqual(["ok", "error"]);
  });
  it("does not mark a tool call successful if a historical result is missing", () => {
    const items = itemsFromMessages([
      { role: "user", content: "open source" },
      { role: "assistant", content: "", tool_calls: [
        { id: "has-result", function: { name: "read_file", arguments: "{}" } },
        { id: "no-result", function: { name: "read_file", arguments: "{}" } },
      ] },
      { role: "tool", tool_call_id: "has-result", content: "" },
    ] as any);
    expect(items.filter((i) => i.kind === "tool").map((i) => i.status)).toEqual(["ok", "unknown"]);
  });

  it("replays an interruption as a terminal state for only unresolved current calls", () => {
    const items = itemsFromMessages([
      { role: "user", content: "first" },
      { role: "assistant", content: "", tool_calls: [
        { id: "first-missing", function: { name: "read_file", arguments: "{}" } },
      ] },
      { role: "user", content: "second" },
      { role: "assistant", content: "", tool_calls: [
        { id: "completed", function: { name: "web_fetch", arguments: "{}" } },
        { id: "interrupted", function: { name: "read_file", arguments: "{}" } },
      ] },
      { role: "tool", tool_call_id: "completed", content: "ok" },
      { role: "notice", kind: "interrupted" },
    ] as any);
    expect(items.filter((i) => i.kind === "tool").map((i) => i.status)).toEqual(["unknown", "ok", "interrupted"]);
    expect(items[items.length - 1]).toMatchObject({ kind: "notice", text: "Interrupted." });
  });

  it("closes only unresolved active calls without mutating the previous transcript", () => {
    const items = [
      { kind: "user", text: "earlier" },
      { kind: "tool", id: "old", name: "read_file", args: {}, status: "…" },
      { kind: "user", text: "current" },
      { kind: "tool", id: "done", name: "web_fetch", args: {}, status: "ok" },
      { kind: "tool", id: "active", name: "read_file", args: {}, status: "…" },
    ] as const;
    const updated = interruptActiveTools([...items]);
    expect(updated.filter((i) => i.kind === "tool").map((i) => i.status)).toEqual(["…", "ok", "interrupted"]);
    expect(items[4].status).toBe("…");
    expect(interruptActiveTools(updated)).toEqual(updated);
  });

});
