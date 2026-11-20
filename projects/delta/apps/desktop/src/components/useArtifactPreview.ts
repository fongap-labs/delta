import { useCallback, useEffect, useRef, useState } from "react";
import { readArtifact, type ArtifactContent } from "../api";

type PreviewState = {
  key: string | null;
  content: ArtifactContent | null;
  error: string | null;
  loading: boolean;
};

/** One preview request owns the result. Older files or sessions cannot overwrite it. */
export function useArtifactPreview(sessionId: string, path?: string) {
  const [state, setState] = useState<PreviewState>({ key: null, content: null, error: null, loading: false });
  const requestId = useRef(0);
  const key = path ? JSON.stringify([sessionId, path]) : null;

  const reload = useCallback(async (): Promise<void> => {
    const id = ++requestId.current;
    if (!path || !key) {
      setState({ key: null, content: null, error: null, loading: false });
      return;
    }
    setState({ key, content: null, error: null, loading: true });
    try {
      const content = await readArtifact(sessionId, path);
      if (id === requestId.current) setState({ key, content, error: null, loading: false });
    } catch (cause) {
      const error = cause instanceof Error ? cause.message : String(cause);
      if (id === requestId.current) setState({ key, content: null, error, loading: false });
    }
  }, [sessionId, path, key]);

  useEffect(() => {
    void reload();
    return () => { requestId.current++; };
  }, [reload]);

  // Render the correct session/file as loading even before its effect runs.
  const current = state.key === key
    ? state
    : { key, content: null, error: null, loading: !!key };
  return { content: current.content, error: current.error, loading: current.loading, reload };
}
