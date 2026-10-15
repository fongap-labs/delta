// A replayed tool row takes its outcome from the stored tool result: a result with an `error` reads
// as a failed step, so it does not turn green again when the conversation is reopened.
import { describe, expect, it } from "vitest";
import { itemsFromMessages } from "./itemsFromMessages";

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
});
