// Maps the raw transcript from GET /v1/sessions/{id}/messages into the GUI's `Item[]` model.
// Extracted from App.tsx so it can be unit-tested without standing up the whole app.
//
// A connector-delivered user message carries a structured `source` sidecar (§3.1); when present it
// becomes a `connector` item (rendered as ConnectorMessageCard) instead of a plain user bubble. This
// generalizes to any connector via the registry — no Slack special-casing here.

import type { ConversationMessage } from "./api";
import type { Attachment, Item } from "./types";

export function itemsFromMessages(messages: ConversationMessage[]): Item[] {
  const items: Item[] = [];
  // Index tool results by tool_call_id so replayed tool rows can show their output
  // (the live view gets this from `tool_finished` events; on replay it's the `role:"tool"` msgs).
  const results: Record<string, string> = {};
  // `_display` sidecar on a tool message = user-facing metadata the agent never saw
  // (e.g. how many hits the privacy filters hid) — surfaces on the tool card.
  const hiddenCounts: Record<string, number> = {};
  // A tool result the runtime stored with an `error` is a failed step, so it replays as one.
  const failed = new Set<string>();
  for (const m of messages || []) {
    if (m.role === "tool" && m.tool_call_id) {
      if (m.error) failed.add(m.tool_call_id);
      results[m.tool_call_id] =
        typeof m.content === "string" ? m.content : JSON.stringify(m.content);
      const hidden = Number(m._display?.hidden_by_filters || 0);
      if (hidden > 0) hiddenCounts[m.tool_call_id] = hidden;
    }
  }
  for (let msgIndex = 0; msgIndex < (messages || []).length; msgIndex++) {
    const m = (messages || [])[msgIndex];
    if (m.role === "user") {
      // Written for the model alone (a web search result the model asked for in text): not something the
      // user said, and not shown in the transcript.
      if (m.model_only) continue;
      // Connector message → structured card; the framed `content` stays for the model, but display
      // renders from the source sidecar.
      if (m.source?.connector) {
        items.push({ kind: "connector", source: m.source });
        continue;
      }
      const user = buildUserItem(m.content);
      // Force-run (`/skill …`): `_display` holds the user's literal line; `content` carries
      // the model-facing framing. Render what the user typed — one truthful bubble.
      if (typeof m._display === "string" && m._display) user.text = m._display;
      // `ts` (unix seconds) is the server's canonical-message stamp; older sessions have none.
      if (typeof m.ts === "number") user.ts = m.ts;
      user.index = msgIndex;
      if (user.text || user.attachments?.length) items.push(user);
    } else if (m.role === "assistant") {
      if (m.content || m.reasoning)
        items.push({
          kind: "assistant",
          text: m.content || "",
          ...(typeof m.ts === "number" ? { ts: m.ts } : {}),
          ...(m.reasoning ? { reasoning: m.reasoning } : {}),
          // The stored message index: a branch ends at this answer.
          index: msgIndex,
        });
      for (const tc of m.tool_calls || []) {
        let args: any = {};
        try {
          args = JSON.parse(tc.function?.arguments || "{}");
        } catch {
          args = {};
        }
        const preview = results[tc.id];
        const hidden = hiddenCounts[tc.id];
        items.push({
          kind: "tool",
          id: tc.id,
          name: tc.function?.name,
          args,
          status: failed.has(tc.id) ? "error" : "ok",
          preview,
          ...(hidden ? { hidden } : {}),
        });
      }
    } else if (m.role === "notice") {
      // Older builds stored each automatic retry as a notice; it was progress, not an outcome.
      if (m.kind === "retrying") continue;
      // Persisted markers (engine `_append_notice`): error/interrupted/model-switch survive
      // reload exactly like the live view rendered them. An error notice is retriable —
      // the Transcript only offers the button when it's the transcript tail.
      items.push(
        m.kind === "interrupted"
          ? { kind: "notice", tone: "warn", text: "Interrupted." }
          : m.kind === "tools_refused"
            ? { kind: "notice", tone: "info", text: String(m.text || "") }
          : m.kind === "model_switch"
            ? { kind: "notice", tone: "info", text: m.text || "Model switched", modelSwitchModel: m.model as string | undefined, modelSwitchImageWarning: !!m.image_warning }
            : m.kind === "compacted"
              ? // The subtle "compacted here" divider (OPE-27) — the transcript itself is intact.
                { kind: "notice", tone: "info", text: m.text || "Context compacted" }
              : { kind: "notice", tone: "warn", text: "Error: " + (m.text || "unknown"), retriable: true },
      );
    }
    // system messages are omitted; tool-result messages are folded into the tool row above
  }
  return items;
}

export function buildUserItem(content: any): Extract<Item, { kind: "user" }> {
  if (typeof content === "string") return { kind: "user", text: content };
  if (!Array.isArray(content)) return { kind: "user", text: "" };

  const text: string[] = [];
  const attachments: Attachment[] = [];
  for (const part of content) {
    if (!part || typeof part !== "object") continue;
    if (part.type === "text" && part.text) {
      text.push(String(part.text));
    } else if (part.type === "image_url") {
      const url = part.image_url?.url;
      if (typeof url === "string" && url.startsWith("data:image/")) {
        attachments.push({ kind: "image", name: "image", data_url: url });
      }
    }
  }
  return { kind: "user", text: text.join("\n\n"), attachments };
}

/**
 * Give optimistic user bubbles (added when the message was sent, before the runtime had stored
 * it) the raw-message index of their stored copy, so Edit works while the run is still going.
 * Matches by text from the newest message backwards; a bubble with no stored twin is left alone.
 */
export function withUserIndexes(current: Item[], persisted: Item[]): Item[] {
  const stored = persisted.filter(
    (it): it is Extract<Item, { kind: "user" }> => it.kind === "user" && it.index !== undefined,
  );
  const taken = new Set<number>(
    current.flatMap((it) => (it.kind === "user" && it.index !== undefined ? [it.index] : [])),
  );
  const out = current.slice();
  for (let i = out.length - 1; i >= 0; i--) {
    const item = out[i];
    if (item.kind !== "user" || item.index !== undefined) continue;
    const twin = [...stored].reverse().find((s) => !taken.has(s.index!) && s.text === item.text);
    if (!twin) continue;
    taken.add(twin.index!);
    out[i] = { ...item, index: twin.index, ...(item.ts === undefined && twin.ts !== undefined ? { ts: twin.ts } : {}) };
  }
  return out;
}

// Tool names of the current request only: everything after the last user message. The progress
// panel counts these, so a new request starts from zero instead of carrying the session's total.
export function turnToolNames(items: Item[]): string[] {
  let from = 0;
  items.forEach((it, i) => {
    if (it.kind === "user") from = i + 1;
  });
  return items.slice(from).flatMap((it) => (it.kind === "tool" ? [it.name] : []));
}
