// Trace state for artifacts, fed by the `artifact_updated` runtime frame.
// The runtime owns status and lineage; this module only reads them and never decides them.

export type ArtifactTraceStatus = "drafting" | "pending_review" | "finalized";

export interface ArtifactSourceRef {
  file_path: string;
  matched_snippet: string;
}

export interface ArtifactTrace {
  path: string;
  name: string;
  status: ArtifactTraceStatus;
  lineage: ArtifactSourceRef[];
  toolCallId: string;
}

const TRACE_STATUSES: ReadonlySet<string> = new Set(["drafting", "pending_review", "finalized"]);

function parseLineage(value: unknown): ArtifactSourceRef[] {
  if (!Array.isArray(value)) return [];
  // A malformed entry is dropped rather than failing the whole artifact.
  return value.flatMap((item): ArtifactSourceRef[] => {
    if (!item || typeof item !== "object") return [];
    const entry = item as Record<string, unknown>;
    if (typeof entry.file_path !== "string" || entry.file_path === "") return [];
    return [
      {
        file_path: entry.file_path,
        matched_snippet: typeof entry.matched_snippet === "string" ? entry.matched_snippet : "",
      },
    ];
  });
}

/** Read one `artifact_updated` frame payload. Returns null when it lacks a usable path or status. */
export function parseArtifactUpdate(payload: Record<string, unknown>): ArtifactTrace | null {
  const artifact = payload.artifact;
  if (!artifact || typeof artifact !== "object") return null;
  const fields = artifact as Record<string, unknown>;
  const path = typeof fields.path === "string" ? fields.path : "";
  const status = typeof fields.status === "string" && TRACE_STATUSES.has(fields.status) ? fields.status : null;
  if (path === "" || status === null) return null;
  return {
    path,
    name: typeof fields.name === "string" && fields.name !== "" ? fields.name : path,
    status: status as ArtifactTraceStatus,
    lineage: parseLineage(fields.lineage),
    toolCallId: typeof payload.tool_call_id === "string" ? payload.tool_call_id : "",
  };
}

/** Replace the trace for the same artifact path, or append it when the path is new. */
export function upsertArtifactTrace(traces: ArtifactTrace[], next: ArtifactTrace): ArtifactTrace[] {
  const index = traces.findIndex((trace) => trace.path === next.path);
  if (index < 0) return [...traces, next];
  return traces.map((trace, position) => (position === index ? next : trace));
}

/** The traces the Trace Logs panel shows: delivered artifacts only. */
export function finalizedArtifactTraces(traces: ArtifactTrace[]): ArtifactTrace[] {
  return traces.filter((trace) => trace.status === "finalized");
}
