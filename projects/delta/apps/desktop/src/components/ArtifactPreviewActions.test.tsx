import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { I18nProvider } from "@delta/i18n/I18nContext";
import type { ReactNode } from "react";
import type { ArtifactInfo } from "../api";
import { ArtifactPreviewActions, ArtifactPreviewFacts } from "./ArtifactPreviewActions";

const { revealMock } = vi.hoisted(() => ({ revealMock: vi.fn() }));
vi.mock("../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../api")>()),
  revealArtifact: (...args: unknown[]) => revealMock(...args),
}));

const artifact: ArtifactInfo = {
  path: "report.pdf", name: "report.pdf", kind: "pdf", size: 0,
  modified_at: 1_800_000_000, abs_path: "/tmp/delta/report.pdf",
};
const wrap = (child: ReactNode) => <I18nProvider locale="en-US">{child}</I18nProvider>;

afterEach(() => { cleanup(); revealMock.mockReset(); });

describe("shared artifact toolbar", () => {
  it("uses existing file actions, with a stable accessible toolbar", () => {
    render(wrap(
      <ArtifactPreviewActions
        sessionId="session-a" artifact={artifact} previewKind="pdf" loading={false}
        onReload={vi.fn(async () => {})} onReloaded={vi.fn()}
      />,
    ));
    expect(screen.getByRole("toolbar", { name: "Preview actions" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Open in default app" }));
    fireEvent.click(screen.getByRole("button", { name: "Show in folder" }));
    expect(revealMock).toHaveBeenNthCalledWith(1, "session-a", "report.pdf", "open");
    expect(revealMock).toHaveBeenNthCalledWith(2, "session-a", "report.pdf", "reveal");
  });

  it("offers reload consistently and disables it while reading", async () => {
    const onReload = vi.fn(async () => {});
    const onReloaded = vi.fn();
    const { rerender } = render(wrap(
      <ArtifactPreviewActions sessionId="s" artifact={artifact} previewKind="markdown" loading={true} onReload={onReload} onReloaded={onReloaded} />,
    ));
    expect(screen.getByRole("button", { name: "Reload preview" }).hasAttribute("disabled")).toBe(true);
    expect(screen.queryByRole("button", { name: "Open in default app" })).toBeNull();
    rerender(wrap(
      <ArtifactPreviewActions sessionId="s" artifact={artifact} previewKind="markdown" loading={false} onReload={onReload} onReloaded={onReloaded} />,
    ));
    fireEvent.click(screen.getByRole("button", { name: "Reload preview" }));
    expect(onReload).toHaveBeenCalledTimes(1);
    await vi.waitFor(() => expect(onReloaded).toHaveBeenCalledTimes(1));
  });
});

describe("recorded artifact facts", () => {
  it("shows real empty-file size, modification time and an explicitly missing validation record", () => {
    render(wrap(<ArtifactPreviewFacts artifact={artifact} checksLookup="loaded" hasRecordedChecks={false} />));
    expect(screen.getByTestId("artifact-file-size").textContent).toBe("0 B");
    expect(screen.getByTestId("artifact-modified-at").getAttribute("datetime")).toBeTruthy();
    expect(screen.getByTestId("artifact-checks-unreported").textContent).toContain("No validation checks recorded");
  });

  it("does not turn a deep-link placeholder or failed lookup into false metadata", () => {
    const minimal: ArtifactInfo = { path: "missing.md", name: "missing.md", kind: "markdown", size: 0, modified_at: 0 };
    const { rerender } = render(wrap(<ArtifactPreviewFacts artifact={minimal} checksLookup="loading" hasRecordedChecks={false} />));
    expect(screen.queryByTestId("artifact-preview-facts")).toBeNull();
    rerender(wrap(<ArtifactPreviewFacts artifact={minimal} checksLookup="unavailable" hasRecordedChecks={false} />));
    expect(screen.queryByTestId("artifact-checks-unreported")).toBeNull();
    rerender(wrap(<ArtifactPreviewFacts artifact={minimal} checksLookup="loaded" hasRecordedChecks={false} />));
    expect(screen.queryByTestId("artifact-file-size")).toBeNull();
    expect(screen.getByTestId("artifact-checks-unreported")).toBeTruthy();
    rerender(wrap(<ArtifactPreviewFacts artifact={artifact} checksLookup="loaded" hasRecordedChecks={true} />));
    expect(screen.queryByTestId("artifact-checks-unreported")).toBeNull();
  });
});
