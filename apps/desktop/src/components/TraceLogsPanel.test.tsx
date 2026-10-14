import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { I18nProvider } from "@delta/i18n/I18nContext";
import { TraceLogsPanel } from "./TraceLogsPanel";
import type { ArtifactTrace } from "../artifactTrace";

afterEach(cleanup);

const trace = (overrides: Partial<ArtifactTrace>): ArtifactTrace => ({
  path: "out/report.md",
  name: "report.md",
  status: "finalized",
  lineage: [],
  toolCallId: "call-1",
  ...overrides,
});

const renderPanel = (traces: ArtifactTrace[]) =>
  render(
    <I18nProvider locale="en-US">
      <TraceLogsPanel traces={traces} />
    </I18nProvider>,
  );

describe("TraceLogsPanel", () => {
  it("renders nothing while no artifact is finalized", () => {
    const { container } = renderPanel([trace({ status: "pending_review" })]);
    expect(container.querySelector(".trace-logs")).toBeNull();
  });

  it("lists source files and their matched snippets for a finalized artifact", () => {
    renderPanel([
      trace({
        lineage: [{ file_path: "in/notes.md", matched_snippet: "Q3 revenue grew 12%" }],
      }),
    ]);
    expect(screen.getByText("Trace Logs")).toBeTruthy();
    expect(screen.getByText("in/notes.md")).toBeTruthy();
    expect(screen.getByText("Q3 revenue grew 12%")).toBeTruthy();
  });

  it("says so when a finalized artifact has no recorded sources", () => {
    renderPanel([trace({ lineage: [] })]);
    expect(screen.getByText("No source files were recorded for this artifact.")).toBeTruthy();
  });

  it("renders snippet text as text, never as markup", () => {
    const { container } = renderPanel([
      trace({ lineage: [{ file_path: "in/x.md", matched_snippet: "<img src=x onerror=alert(1)>" }] }),
    ]);
    expect(container.querySelector("img")).toBeNull();
    expect(screen.getByText("<img src=x onerror=alert(1)>")).toBeTruthy();
  });
});
