import type { Item } from "./types";
import { toolDurationMs } from "./toolTiming";

/** Counts recorded tool rows in this conversation, not model estimates or future work. */
export interface ActivityMetrics {
  total: number;
  completed: number;
  failed: number;
  running: number;
  interrupted: number;
  unknown: number;
  timed: number;
  longestMs: number | null;
}

/** A measured tool interval is not the elapsed duration of a whole Agent task. */
export function summarizeActivity(items: readonly Item[]): ActivityMetrics {
  const stats: ActivityMetrics = {
    total: 0,
    completed: 0,
    failed: 0,
    running: 0,
    interrupted: 0,
    unknown: 0,
    timed: 0,
    longestMs: null,
  };
  for (const item of items) {
    if (item.kind !== "tool") continue;
    stats.total++;
    switch (item.status) {
      case "ok": stats.completed++; break;
      case "error": stats.failed++; break;
      case "…": stats.running++; break;
      case "interrupted": stats.interrupted++; break;
      default: stats.unknown++; break;
    }
    const duration = toolDurationMs(item);
    if (duration !== null) {
      stats.timed++;
      stats.longestMs = stats.longestMs === null ? duration : Math.max(stats.longestMs, duration);
    }
  }
  return stats;
}
