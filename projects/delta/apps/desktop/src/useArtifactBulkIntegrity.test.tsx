import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import type { ArtifactInfo } from "./api";
import { summarizeBulkIntegrity, useBulkIntegrity } from "./useArtifactBulkIntegrity";

const { verifyMock } = vi.hoisted(() => ({ verifyMock: vi.fn() }));
vi.mock("./api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./api")>()),
  verifyArtifactIntegrity: (...args: unknown[]) => verifyMock(...args),
}));

const files = (paths: string[]): ArtifactInfo[] => paths.map((path) => ({
  path, name: path, kind: "markdown", size: 3, modified_at: 1_800_000_000,
}));
afterEach(() => { cleanup(); verifyMock.mockReset(); });

describe("useBulkIntegrity", () => {
  it("does not read without a click and checks unique paths sequentially", async () => {
    const paths = files(["a.md", "a.md", "b.md", "c.md"]);
    let resolveFirst!: (value: unknown) => void;
    const first = new Promise((resolve) => { resolveFirst = resolve; });
    verifyMock.mockImplementationOnce(() => first)
      .mockResolvedValueOnce({ ok: true, path: "b.md", status: "changed" })
      .mockResolvedValueOnce({ ok: true, path: "c.md", status: "unrecorded" });
    const { result } = renderHook(() => useBulkIntegrity("s1", paths, true));
    expect(result.current.phase).toBe("idle");
    expect(verifyMock).not.toHaveBeenCalled();
    act(() => result.current.start());
    expect(result.current.total).toBe(3);
    expect(verifyMock).toHaveBeenCalledTimes(1);
    await act(async () => { resolveFirst({ ok: true, path: "a.md", status: "matched" }); await first; });
    await waitFor(() => expect(result.current.phase).toBe("complete"));
    expect(verifyMock.mock.calls.map((args) => args[1])).toEqual(["a.md", "b.md", "c.md"]);
    expect(result.current.checked).toBe(3);
    expect(summarizeBulkIntegrity(result.current.results)).toEqual({
      matched: 1, changed: 1, unrecorded: 1, unsupported: 0, unavailable: 0,
    });
  });

  it("stops after the in-flight request and never checks further files", async () => {
    let resolveFirst!: (value: unknown) => void;
    const first = new Promise((resolve) => { resolveFirst = resolve; });
    verifyMock.mockReturnValueOnce(first);
    const artifacts = files(["a.md", "b.md"]);
    const { result } = renderHook(() => useBulkIntegrity("s1", artifacts, true));
    act(() => result.current.start());
    act(() => result.current.stop());
    await act(async () => { resolveFirst({ ok: true, path: "a.md", status: "matched" }); await first; });
    expect(result.current.phase).toBe("stopped");
    expect(result.current.checked).toBe(0);
    expect(verifyMock).toHaveBeenCalledTimes(1);
  });

  it("invalidates old-session results, including identical relative paths", async () => {
    let resolveOld!: (value: unknown) => void;
    const old = new Promise((resolve) => { resolveOld = resolve; });
    verifyMock.mockReturnValueOnce(old)
      .mockResolvedValueOnce({ ok: true, path: "report.md", status: "changed" });
    const firstFiles = files(["report.md"]);
    const secondFiles = files(["report.md"]);
    const { result, rerender } = renderHook(({ sessionId, artifacts }) =>
      useBulkIntegrity(sessionId, artifacts, true), {
      initialProps: { sessionId: "s1", artifacts: firstFiles },
    });
    act(() => result.current.start());
    rerender({ sessionId: "s2", artifacts: secondFiles });
    expect(result.current.phase).toBe("idle");
    act(() => result.current.start());
    await waitFor(() => expect(result.current.results["report.md"]).toBe("changed"));
    await act(async () => { resolveOld({ ok: true, path: "report.md", status: "matched" }); await old; });
    expect(result.current.results["report.md"]).toBe("changed");
  });

  it("distinguishes failed requests and invalid response paths from mismatches", async () => {
    verifyMock.mockRejectedValueOnce(new Error("Unavailable"))
      .mockResolvedValueOnce({ ok: true, path: "wrong.md", status: "matched" });
    const artifacts = files(["a.md", "b.md"]);
    const { result } = renderHook(() => useBulkIntegrity("s1", artifacts, true));
    act(() => result.current.start());
    await waitFor(() => expect(result.current.phase).toBe("complete"));
    expect(result.current.results).toEqual({ "a.md": "unavailable", "b.md": "unavailable" });
  });
});
