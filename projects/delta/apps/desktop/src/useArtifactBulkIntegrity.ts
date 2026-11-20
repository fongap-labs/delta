import { useEffect, useRef, useState } from "react";
import { verifyArtifactIntegrity, type ArtifactInfo, type ArtifactIntegrityStatus } from "./api";

export type BulkIntegrityStatus = ArtifactIntegrityStatus | "unavailable";
export type BulkIntegrityPhase = "idle" | "checking" | "complete" | "stopped";
export type BulkIntegrityResults = Record<string, BulkIntegrityStatus>;

interface BulkIntegrityState {
  sessionId: string;
  artifacts: readonly ArtifactInfo[];
  phase: BulkIntegrityPhase;
  results: BulkIntegrityResults;
  checked: number;
  total: number;
}

const empty: BulkIntegrityState = {
  sessionId: "", artifacts: [], phase: "idle", results: {}, checked: 0, total: 0,
};

function resultStatus(result: Awaited<ReturnType<typeof verifyArtifactIntegrity>>, path: string): BulkIntegrityStatus {
  if (!result.ok || result.path !== path) return "unavailable";
  switch (result.status) {
    case "matched":
    case "changed":
    case "unrecorded":
    case "unsupported":
      return result.status;
    default:
      return "unavailable";
  }
}

/**
 * One explicit request at a time, reusing Core's registered-path / SHA-256 checks.
 * Switching session, refreshing the file list, or closing the panel invalidates
 * pending results. Stopping prevents any further file requests after the current one.
 */
export function useBulkIntegrity(sessionId: string, artifacts: readonly ArtifactInfo[], isEnabled: boolean) {
  const [state, setState] = useState<BulkIntegrityState>(empty);
  const generation = useRef(0);
  useEffect(() => {
    generation.current++;
    setState({ sessionId, artifacts, phase: "idle", results: {}, checked: 0, total: 0 });
    return () => { generation.current++; };
  }, [sessionId, artifacts, isEnabled]);

  const isCurrent = isEnabled && state.sessionId === sessionId && state.artifacts === artifacts;
  const visible = isCurrent ? state : {
    sessionId, artifacts, phase: "idle" as const, results: {}, checked: 0, total: 0,
  };

  const start = () => {
    if (!isEnabled || artifacts.length === 0 || visible.phase === "checking") return;
    const paths = [...new Set(artifacts.map((artifact) => artifact.path).filter((path) => path.length > 0))];
    if (paths.length === 0) return;
    const id = ++generation.current;
    setState({ sessionId, artifacts, phase: "checking", results: {}, checked: 0, total: paths.length });
    void (async () => {
      for (const path of paths) {
        if (id !== generation.current) return;
        let status: BulkIntegrityStatus = "unavailable";
        try {
          status = resultStatus(await verifyArtifactIntegrity(sessionId, path), path);
        } catch {
          // A failed read is not a hash mismatch and must not masquerade as a match.
        }
        if (id !== generation.current) return;
        setState((current) => {
          if (current.sessionId !== sessionId || current.artifacts !== artifacts || current.phase !== "checking") return current;
          const checked = current.checked + 1;
          return {
            ...current,
            checked,
            results: { ...current.results, [path]: status },
            phase: checked === paths.length ? "complete" : "checking",
          };
        });
      }
    })();
  };

  const stop = () => {
    if (visible.phase !== "checking") return;
    generation.current++;
    setState((current) => current.sessionId === sessionId && current.artifacts === artifacts && current.phase === "checking"
      ? { ...current, phase: "stopped" } : current);
  };

  return { ...visible, start, stop };
}

export function summarizeBulkIntegrity(results: BulkIntegrityResults) {
  const counts: Record<BulkIntegrityStatus, number> = {
    matched: 0, changed: 0, unrecorded: 0, unsupported: 0, unavailable: 0,
  };
  for (const status of Object.values(results)) counts[status]++;
  return counts;
}
