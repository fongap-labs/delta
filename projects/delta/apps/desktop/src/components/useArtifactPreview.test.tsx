import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { useArtifactPreview } from "./useArtifactPreview";

const { readMock } = vi.hoisted(() => ({ readMock: vi.fn() }));
vi.mock("../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../api")>()),
  readArtifact: (...args: unknown[]) => readMock(...args),
}));

afterEach(() => {
  cleanup();
  readMock.mockReset();
});

describe("useArtifactPreview", () => {
  it("never shows a prior file's result after a rapid selection change", async () => {
    let resolveOld!: (value: unknown) => void;
    const old = new Promise((resolve) => { resolveOld = resolve; });
    readMock.mockImplementation((_session: string, path: string) =>
      path === "first.md" ? old : Promise.resolve({ kind: "markdown", content: "second file" }));
    const { result, rerender } = renderHook(({ path }) => useArtifactPreview("s1", path), {
      initialProps: { path: "first.md" },
    });
    rerender({ path: "second.md" });
    await waitFor(() => expect(result.current.content?.content).toBe("second file"));
    await act(async () => { resolveOld({ kind: "markdown", content: "stale first" }); await old; });
    expect(result.current.content?.content).toBe("second file");
    expect(result.current.error).toBeNull();
  });

  it("isolates the same relative artifact path across different sessions", async () => {
    let resolveFirst!: (value: unknown) => void;
    const first = new Promise((resolve) => { resolveFirst = resolve; });
    readMock.mockImplementation((session: string) =>
      session === "s1" ? first : Promise.resolve({ kind: "markdown", content: "second session" }));
    const { result, rerender } = renderHook(({ session }) => useArtifactPreview(session, "report.md"), {
      initialProps: { session: "s1" },
    });
    rerender({ session: "s2" });
    await waitFor(() => expect(result.current.content?.content).toBe("second session"));
    await act(async () => { resolveFirst({ kind: "markdown", content: "stale session" }); await first; });
    expect(result.current.content?.content).toBe("second session");
  });

  it("reports a rejected read, then retries without keeping the previous error", async () => {
    readMock.mockRejectedValueOnce(new Error("File unavailable"))
      .mockResolvedValueOnce({ kind: "markdown", content: "Recovered" });
    const { result } = renderHook(() => useArtifactPreview("s1", "report.md"));
    await waitFor(() => expect(result.current.error).toBe("File unavailable"));
    expect(result.current.loading).toBe(false);
    expect(result.current.content).toBeNull();
    await act(async () => { await result.current.reload(); });
    expect(result.current.error).toBeNull();
    expect(result.current.content?.content).toBe("Recovered");
    expect(readMock).toHaveBeenCalledTimes(2);
  });

  it("keeps a missing selection empty without calling the read API", async () => {
    const { result } = renderHook(() => useArtifactPreview("s1"));
    expect(result.current.content).toBeNull();
    expect(result.current.loading).toBe(false);
    expect(readMock).not.toHaveBeenCalled();
  });
});
