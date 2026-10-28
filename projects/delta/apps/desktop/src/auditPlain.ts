import type { AuditEvent } from "./api";

// Turns one activity record into plain words for someone who does not read logs. The record stays
// the source of truth; this only decides how it is said.

/** The arguments as an object. Records store them as JSON text, so parse that; anything else is empty. */
export function auditArgs(args: unknown): Record<string, unknown> {
  if (args && typeof args === "object" && !Array.isArray(args)) return args as Record<string, unknown>;
  if (typeof args === "string") {
    try {
      const parsed: unknown = JSON.parse(args);
      if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) return parsed as Record<string, unknown>;
    } catch {
      // Not JSON: nothing readable to show.
    }
  }
  return {};
}

export type AuditLine = { key: string; params: Record<string, string> };

const SHORT = 80;
const clip = (text: string) => (text.length > SHORT ? `${text.slice(0, SHORT)}…` : text);

/** The one-sentence description of what happened, as a translation key and its values. */
export function auditLine(ev: { tool: string; args: unknown }): AuditLine {
  const args = auditArgs(ev.args);
  if (ev.tool === "web_search" && typeof args.query === "string") {
    return { key: "activity.plain.search", params: { query: clip(args.query) } };
  }
  if (ev.tool === "web_fetch" && typeof args.url === "string") {
    return { key: "activity.plain.fetch", params: { url: clip(args.url) } };
  }
  return { key: "activity.plain.tool", params: { tool: ev.tool || "?" } };
}

export type AuditState = "waiting" | "approved" | "denied" | "done" | "failed" | "other";

/** Where the action stands, in the user's terms. */
export function auditState(ev: Pick<AuditEvent, "stage" | "status">): AuditState {
  const status = (ev.status || "").toLowerCase();
  const stage = (ev.stage || "").toLowerCase();
  if (status === "pending" || stage === "approval_required") return "waiting";
  if (status === "denied" || status === "rejected") return "denied";
  if (status === "approved" || stage === "approval_resolved") return "approved";
  if (status === "failed" || status === "error" || stage === "failed") return "failed";
  if (["completed", "succeeded", "ok", "done", "executed"].includes(status) || stage === "completed") return "done";
  return "other";
}

/** Local time for today, otherwise month, day and time. Falls back to the raw text. */
export function auditWhen(timestamp: string, now: Date = new Date()): string {
  // Records carry nanosecond fractions; the Date parser takes milliseconds.
  const ms = Date.parse(timestamp.replace(/(\.\d{3})\d+/, "$1"));
  if (Number.isNaN(ms)) return timestamp;
  const when = new Date(ms);
  const sameDay = when.toDateString() === now.toDateString();
  return sameDay
    ? when.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
    : when.toLocaleString([], { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" });
}
