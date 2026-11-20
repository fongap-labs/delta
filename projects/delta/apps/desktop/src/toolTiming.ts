/** Runtime-emitted timings only. Never estimate from local event arrival. */
import type { Item } from "./types";

type ToolItem = Extract<Item, { kind: "tool" }>;

export function readEventMs(value: unknown): number | undefined {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0 ? value : undefined;
}

export function markToolStart(items: Item[], id: unknown, atMs: unknown): Item[] {
  const start = readEventMs(atMs);
  if (typeof id !== "string" || !id || start === undefined) return items;
  for (let i = items.length - 1; i >= 0; i--) {
    const item = items[i];
    if (item.kind !== "tool" || item.id !== id || item.status !== "…") continue;
    if (item.startedAtMs !== undefined) return items;
    const next = [...items];
    next[i] = { ...item, startedAtMs: start };
    return next;
  }
  return items;
}

interface ToolFinish {
  id?: unknown;
  name: string;
  status: "ok" | "error";
  preview?: string;
  hidden?: number;
  standingRule?: string;
  atMs?: unknown;
}

/** Match by runtime call ID; old producers lacking IDs fall back to the last pending name. */
export function finishToolCall(items: Item[], event: ToolFinish): Item[] {
  const hasId = typeof event.id === "string" && event.id.length > 0;
  for (let i = items.length - 1; i >= 0; i--) {
    const item = items[i];
    if (item.kind !== "tool" || item.status !== "…") continue;
    if (hasId ? item.id !== event.id : item.name !== event.name) continue;
    const finish = readEventMs(event.atMs);
    const start = readEventMs(item.startedAtMs);
    const validFinish = start !== undefined && finish !== undefined && finish >= start;
    const next = [...items];
    next[i] = {
      ...item,
      status: event.status,
      preview: event.preview,
      ...(event.hidden ? { hidden: event.hidden } : {}),
      ...(event.standingRule ? { standingRule: event.standingRule } : {}),
      ...(validFinish ? { finishedAtMs: finish } : {}),
    };
    return next;
  }
  return items;
}

export function toolDurationMs(item: ToolItem): number | null {
  if (item.status !== "ok" && item.status !== "error") return null;
  const start = readEventMs(item.startedAtMs);
  const end = readEventMs(item.finishedAtMs);
  return start !== undefined && end !== undefined && end >= start ? end - start : null;
}

export function formatToolDuration(ms: number): string {
  if (ms < 1000) return String(Math.round(ms)) + " ms";
  if (ms < 60_000) return (ms / 1000).toFixed(1).replace(/\.0$/, "") + " s";
  return String(Math.floor(ms / 60_000)) + "m " + String(Math.floor((ms % 60_000) / 1000)) + "s";
}
