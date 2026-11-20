import { describe, expect, it } from "vitest";
import type { ArtifactInfo } from "./api";
import { reviewArtifactInventory } from "./artifactInventory";

const files = (paths: string[]): ArtifactInfo[] => paths.map((path) => ({
  path, name: path, kind: "markdown", size: 42, modified_at: 1_800_000_000,
}));
const output = (path: string, validation?: unknown, previous_version?: string) => ({
  role: "tool", content: {
    artifacts: [{ path, ...(previous_version ? { previous_version } : {}) }],
    ...(validation ? { validation } : {}),
  },
});

describe("reviewArtifactInventory", () => {
  it("counts real validation verdicts and earlier-version references only", () => {
    const v = reviewArtifactInventory(
      files(["a.md", "b.md", "c.md", "d.md"]),
      [
        output("a.md", { ok: true, checks: [{ name: "format", ok: true }] }, ".delta/versions/a.md"),
        output("b.md", { ok: false, checks: [{ name: "format", ok: false }] }),
        output("c.md"),
      ],
    );
    expect(v).toEqual({
      byPath: { "a.md": "passed", "b.md": "failed", "c.md": "unrecorded", "d.md": "unrecorded" },
      passed: 1, failed: 1, unrecorded: 2, priorVersions: 1,
    });
  });

  it("uses the newest producing result for the same path and deduplicates list rows", () => {
    const v = reviewArtifactInventory(files(["a.md", "a.md"]), [
      output("a.md", { ok: false, checks: [{ name: "old", ok: false }] }, ".delta/versions/old"),
      output("a.md", { ok: true, checks: [{ name: "new", ok: true }] }),
    ]);
    expect(v).toEqual({ byPath: { "a.md": "passed" }, passed: 1, failed: 0, unrecorded: 0, priorVersions: 0 });
  });

  it("never interprets a verdict without individual recorded checks as a pass", () => {
    const v = reviewArtifactInventory(files(["a.md", "b.md"]), [
      output("a.md", { ok: true, checks: [] }),
      output("b.md", { ok: true }),
    ]);
    expect(v.passed).toBe(0);
    expect(v.unrecorded).toBe(2);
  });
});
