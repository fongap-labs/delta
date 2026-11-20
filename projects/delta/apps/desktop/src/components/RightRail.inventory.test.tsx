import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { I18nProvider } from "@delta/i18n/I18nContext";
import { RightRail } from "./RightRail";
import type { ReactNode } from "react";

const { getArtifactsMock, getSessionMessagesMock, verifyArtifactMock } = vi.hoisted(() => ({
  getArtifactsMock: vi.fn(),
  getSessionMessagesMock: vi.fn(),
  verifyArtifactMock: vi.fn(),
}));
vi.mock("../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../api")>()),
  getArtifacts: (...args: unknown[]) => getArtifactsMock(...args),
  getSessionMessages: (...args: unknown[]) => getSessionMessagesMock(...args),
  verifyArtifactIntegrity: (...args: unknown[]) => verifyArtifactMock(...args),
}));

const props = (extra: Record<string, unknown> = {}) => ({
  active: true, sessionId: "s1", refreshKey: 0,
  toolNames: [], todo: [] as never[], running: false, showArtifacts: true, ...extra,
});
const wrap = (element: ReactNode) => <I18nProvider locale="en-US">{element}</I18nProvider>;
const info = (path: string) => ({
  path, name: path, abs_path: "/workspace/" + path, kind: "markdown", size: 10, modified_at: 1_800_000_000,
});
const produced = (path: string, isPassed: boolean) => ({
  role: "tool", content: {
    artifacts: [{ path }],
    validation: { ok: isPassed, checks: [{ name: "output", ok: isPassed }] },
  },
});

afterEach(cleanup);
beforeEach(() => { getArtifactsMock.mockReset(); getSessionMessagesMock.mockReset(); verifyArtifactMock.mockReset(); });

describe("RightRail artifact inventory", () => {
  it("reports historical states and filters attention-worthy files without hiding the navigation", async () => {
    getArtifactsMock.mockResolvedValue([info("passed.md"), info("failed.md"), info("unknown.md")]);
    getSessionMessagesMock.mockResolvedValue([produced("passed.md", true), produced("failed.md", false)]);
    render(wrap(<RightRail {...props()} />));
    expect((await screen.findByTestId("artifact-inventory-counts")).textContent).toContain("Recorded pass 1");
    const statuses = screen.getAllByTestId("artifact-review-status");
    expect(statuses.map((item) => item.getAttribute("data-state"))).toEqual(["passed", "failed", "unrecorded"]);
    const group = screen.getByRole("group", { name: "Filter artifact checks" });
    fireEvent.click(within(group).getByRole("button", { name: "Needs review" }));
    expect(screen.queryByRole("button", { name: /passed\.md/i })).toBeNull();
    expect(screen.getByRole("button", { name: /failed\.md/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /unknown\.md/i })).toBeTruthy();
    fireEvent.click(within(group).getByRole("button", { name: "All" }));
    expect(screen.getByRole("button", { name: /passed\.md/i })).toBeTruthy();
  });

  it("runs batch checks only on request, then displays current-file exceptions", async () => {
    getArtifactsMock.mockResolvedValue([info("a.md"), info("b.md")]);
    getSessionMessagesMock.mockResolvedValue([]);
    verifyArtifactMock.mockResolvedValueOnce({ ok: true, path: "a.md", status: "matched" })
      .mockResolvedValueOnce({ ok: true, path: "b.md", status: "changed" });
    render(wrap(<RightRail {...props()} />));
    await screen.findByTestId("artifact-inventory-counts");
    expect(verifyArtifactMock).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Check files" }));
    await waitFor(() => expect(screen.getByTestId("artifact-bulk-progress").textContent).toContain("2/2"));
    expect(verifyArtifactMock.mock.calls.map((args) => args[1])).toEqual(["a.md", "b.md"]);
    expect(screen.getByTestId("artifact-bulk-counts").textContent).toContain("Changed 1");
    expect(screen.getAllByTestId("artifact-bulk-status").map((item) => item.getAttribute("data-state")))
      .toEqual(["matched", "changed"]);
    expect(screen.getByRole("button", { name: /b\.md/i })).toBeTruthy();
  });

  it("does not label files as missing checks if the session evidence request fails", async () => {
    getArtifactsMock.mockResolvedValue([info("report.md")]);
    getSessionMessagesMock.mockRejectedValue(new Error("Unavailable"));
    render(wrap(<RightRail {...props()} />));
    expect(await screen.findByTestId("artifact-inventory-unavailable")).toBeTruthy();
    expect(screen.queryByTestId("artifact-review-status")).toBeNull();
  });

  it("does not silently truncate after 16 files and resets with a new session", async () => {
    getArtifactsMock.mockImplementation(async (session: string) =>
      session === "s1" ? Array.from({ length: 18 }, (_, i) => info("file-" + i + ".md")) : [info("next.md")],
    );
    getSessionMessagesMock.mockResolvedValue([]);
    const { rerender } = render(wrap(<RightRail {...props()} />));
    expect(await screen.findByTestId("artifact-inventory-counts")).toBeTruthy();
    expect(screen.getAllByRole("button", { name: /file-\d+\.md/i })).toHaveLength(16);
    fireEvent.click(screen.getByTestId("artifact-inventory-expand"));
    expect(screen.getAllByRole("button", { name: /file-\d+\.md/i })).toHaveLength(18);

    rerender(wrap(<RightRail {...props({ sessionId: "s2" })} />));
    await waitFor(() => expect(screen.getByRole("button", { name: /next\.md/i })).toBeTruthy());
    expect(screen.queryByRole("button", { name: /file-0\.md/i })).toBeNull();
  });
});
