import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { I18nProvider } from "@delta/i18n/I18nContext";
import type { ReactNode } from "react";
import { ArtifactIntegrityAction } from "./ArtifactIntegrityAction";

const { verifyMock } = vi.hoisted(() => ({ verifyMock: vi.fn() }));
vi.mock("../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../api")>()),
  verifyArtifactIntegrity: (...args: unknown[]) => verifyMock(...args),
}));
const wrap = (content: ReactNode) => <I18nProvider locale="en-US">{content}</I18nProvider>;
afterEach(() => { cleanup(); verifyMock.mockReset(); });

describe("ArtifactIntegrityAction", () => {
  it("runs only when requested and identifies a matching file without approving delivery", async () => {
    verifyMock.mockResolvedValue({ ok: true, path: "report.md", status: "matched" });
    render(wrap(<ArtifactIntegrityAction sessionId="s1" path="report.md" />));
    expect(screen.getByTestId("artifact-integrity-status").getAttribute("data-state")).toBe("idle");
    expect(verifyMock).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Check now" }));
    await waitFor(() => expect(screen.getByTestId("artifact-integrity-status").getAttribute("data-state")).toBe("matched"));
    expect(verifyMock).toHaveBeenCalledWith("s1", "report.md");
    expect(screen.getByTestId("artifact-integrity-status").textContent).toContain("at check time");
    expect(screen.queryByText(/approved for delivery/i)).toBeNull();
  });

  it("distinguishes changed, missing digest, and unavailable", async () => {
    verifyMock.mockResolvedValueOnce({ ok: true, path: "report.md", status: "changed" })
      .mockResolvedValueOnce({ ok: true, path: "report.md", status: "unrecorded" })
      .mockResolvedValueOnce({ ok: false, error: "File gone" });
    render(wrap(<ArtifactIntegrityAction sessionId="s1" path="report.md" />));
    const check = () => fireEvent.click(screen.getByRole("button", { name: "Check now" }));
    check();
    await waitFor(() => expect(screen.getByTestId("artifact-integrity-status").getAttribute("data-state")).toBe("changed"));
    check();
    await waitFor(() => expect(screen.getByTestId("artifact-integrity-status").getAttribute("data-state")).toBe("unrecorded"));
    check();
    await waitFor(() => expect(screen.getByTestId("artifact-integrity-status").getAttribute("data-state")).toBe("unavailable"));
  });

  it("rejects a reply for another artifact and keeps only the latest check result", async () => {
    let resolveFirst!: (value: unknown) => void;
    const first = new Promise((resolve) => { resolveFirst = resolve; });
    verifyMock.mockImplementationOnce(() => first)
      .mockResolvedValueOnce({ ok: true, path: "report.md", status: "changed" });
    const { rerender } = render(wrap(<ArtifactIntegrityAction key="s1:report.md" sessionId="s1" path="report.md" />));
    fireEvent.click(screen.getByRole("button", { name: "Check now" }));
    expect(screen.getByRole("button", { name: "Check now" }).hasAttribute("disabled")).toBe(true);
    rerender(wrap(<ArtifactIntegrityAction key="s2:report.md" sessionId="s2" path="report.md" />));
    fireEvent.click(screen.getByRole("button", { name: "Check now" }));
    await waitFor(() => expect(screen.getByTestId("artifact-integrity-status").getAttribute("data-state")).toBe("changed"));
    resolveFirst({ ok: true, path: "report.md", status: "matched" });
    await waitFor(() => expect(screen.getByTestId("artifact-integrity-status").getAttribute("data-state")).toBe("changed"));
  });
});
