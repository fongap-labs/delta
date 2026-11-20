import { useEffect, useState } from "react";
import { getSessionMessages, type ArtifactInfo } from "./api";
import { reviewArtifactInventory, type ArtifactInventoryReview } from "./artifactInventory";

type Lookup = "idle" | "loading" | "loaded" | "unavailable";
interface InventoryState {
  sessionId: string;
  artifacts: readonly ArtifactInfo[];
  phase: Lookup;
  review: ArtifactInventoryReview | null;
}
const empty: InventoryState = { sessionId: "", artifacts: [], phase: "idle", review: null };

/** Session-scoped, cancel-safe read of historical artifact evidence. */
export function useArtifactInventory(sessionId: string, artifacts: readonly ArtifactInfo[], isEnabled: boolean) {
  const [state, setState] = useState<InventoryState>(empty);
  useEffect(() => {
    if (!isEnabled || artifacts.length === 0) return;
    let isCurrent = true;
    setState({ sessionId, artifacts, phase: "loading", review: null });
    getSessionMessages(sessionId)
      .then((messages) => {
        if (isCurrent) setState({ sessionId, artifacts, phase: "loaded", review: reviewArtifactInventory(artifacts, messages) });
      })
      .catch(() => {
        if (isCurrent) setState({ sessionId, artifacts, phase: "unavailable", review: null });
      });
    return () => { isCurrent = false; };
  }, [sessionId, artifacts, isEnabled]);

  if (!isEnabled || artifacts.length === 0) return empty;
  // Never let the previous session/list briefly masquerade as the current one.
  if (state.sessionId !== sessionId || state.artifacts !== artifacts) {
    return { sessionId, artifacts, phase: "loading" as const, review: null };
  }
  return state;
}
