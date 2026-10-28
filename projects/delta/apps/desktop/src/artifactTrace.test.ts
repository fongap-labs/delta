import { describe, expect, it } from "vitest";
import {
  finalizedArtifactTraces,
  parseArtifactUpdate,
  upsertArtifactTrace,
  type ArtifactTrace,
} from "./artifactTrace";

const frame = (artifact: Record<string, unknown>) => ({ tool_call_id: "call-1", artifact });

describe("parseArtifactUpdate", () => {
  it("reads status, name and lineage from a finalized frame", () => {
    const trace = parseArtifactUpdate(
      frame({
        path: "out/report.md",
        name: "report.md",
        status: "finalized",
        lineage: [{ file_path: "in/notes.md", matched_snippet: "Q3 revenue grew" }],
      }),
    );
    expect(trace).toEqual({
      path: "out/report.md",
      name: "report.md",
      status: "finalized",
      lineage: [{ file_path: "in/notes.md", matched_snippet: "Q3 revenue grew" }],
      toolCallId: "call-1",
    });
  });

  it("treats a pre-status record as no trace, not as a guessed state", () => {
    // Records written before statuses existed carry no status field.
    expect(parseArtifactUpdate(frame({ path: "out/old.md", name: "old.md" }))).toBeNull();
  });

  it("rejects an unknown status and a missing path", () => {
    expect(parseArtifactUpdate(frame({ path: "out/a.md", status: "Finalized" }))).toBeNull();
    expect(parseArtifactUpdate(frame({ status: "finalized" }))).toBeNull();
    expect(parseArtifactUpdate({ tool_call_id: "x" })).toBeNull();
  });

  it("drops malformed lineage entries without failing the artifact", () => {
    const trace = parseArtifactUpdate(
      frame({
        path: "out/a.md",
        status: "pending_review",
        lineage: [null, "text", { matched_snippet: "no path" }, { file_path: "in/b.md" }],
      }),
    );
    expect(trace?.lineage).toEqual([{ file_path: "in/b.md", matched_snippet: "" }]);
  });

  it("falls back to the path when the name is absent", () => {
    expect(parseArtifactUpdate(frame({ path: "out/a.md", status: "drafting" }))?.name).toBe("out/a.md");
  });
});

describe("upsertArtifactTrace and finalizedArtifactTraces", () => {
  const base = (path: string, status: ArtifactTrace["status"]): ArtifactTrace => ({
    path,
    name: path,
    status,
    lineage: [],
    toolCallId: "call",
  });

  it("replaces the entry for the same path when its status changes", () => {
    const first = upsertArtifactTrace([], base("out/a.md", "pending_review"));
    const second = upsertArtifactTrace(first, base("out/a.md", "finalized"));
    expect(second).toHaveLength(1);
    expect(second[0].status).toBe("finalized");
  });

  it("shows only finalized artifacts in the trace list", () => {
    const traces = [base("out/a.md", "drafting"), base("out/b.md", "finalized"), base("out/c.md", "pending_review")];
    expect(finalizedArtifactTraces(traces).map((trace) => trace.path)).toEqual(["out/b.md"]);
  });
});
