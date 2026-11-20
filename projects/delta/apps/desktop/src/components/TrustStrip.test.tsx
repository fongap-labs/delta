import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { I18nProvider } from "@delta/i18n/I18nContext";
import { TrustStrip } from "./TrustStrip";
import type { ArtifactTrust } from "../artifactTrust";
import type { RunSource } from "../api";

const TRUST: ArtifactTrust = {
  producedBy: "office_worker",
  sha256: "a".repeat(64),
  checks: [
    { name: "format", isOk: true, detail: "Valid" },
    { name: "references", isOk: false, detail: "Missing link" },
  ],
  isValidationOk: false,
};
const SOURCES: RunSource[] = [{
  source_id: "src-1",
  origin: "web",
  location: "https://example.org/report?q=private-token#section",
  fingerprint: "cafebabedeadbeef",
  read_at: "2026-11-01T12:00:00Z",
  cited: true,
}];
function renderTrust(trust: ArtifactTrust, sources: RunSource[] = []) {
  return render(
    <I18nProvider locale="en-US">
      <TrustStrip trust={trust} sources={sources} />
    </I18nProvider>,
  );
}
afterEach(cleanup);

describe("artifact trust disclosure controls", () => {
  it("shows only recorded facts in their original order and reveals check details on demand", () => {
    renderTrust(TRUST);
    const strip = screen.getByTestId("trust-strip");
    const verdict = screen.getByTestId("trust-verdict");
    expect(verdict.className).toContain("ds-button-ghost");
    expect(verdict.getAttribute("type")).toBe("button");
    expect(verdict.getAttribute("aria-expanded")).toBe("false");
    expect(within(strip).queryByTestId("trust-checks")).toBeNull();
    expect(screen.getByTestId("trust-producer").textContent).toContain("office_worker");
    expect(screen.getByTestId("trust-checksum").textContent).toContain("aaaaaaaa");
    fireEvent.click(verdict);
    const checks = screen.getByTestId("trust-checks");
    expect(verdict.getAttribute("aria-expanded")).toBe("true");
    expect(verdict.getAttribute("aria-controls")).toBe(checks.id);
    expect(within(checks).getByText("format")).toBeTruthy();
    expect(within(checks).getByText("references")).toBeTruthy();
    fireEvent.click(verdict);
    expect(screen.queryByTestId("trust-checks")).toBeNull();
  });

  it("strips query and fragment from both source text and hover title", () => {
    renderTrust({ producedBy: "research_worker" }, SOURCES);
    expect(screen.queryByTestId("trust-verdict")).toBeNull();
    const button = screen.getByTestId("trust-sources");
    expect(button.className).toContain("ds-button-ghost");
    expect(button.getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(button);
    const list = screen.getByTestId("trust-source-list");
    expect(button.getAttribute("aria-controls")).toBe(list.id);
    const label = within(list).getByText("https://example.org/report");
    expect(label.getAttribute("title")).toBe("https://example.org/report");
    expect(list.textContent).not.toContain("private-token");
    expect(list.textContent).toContain("Cited");
  });

  it("does not create trust facts when none were recorded", () => {
    const { container } = renderTrust({});
    expect(container.textContent).toBe("");
    expect(screen.queryByTestId("trust-strip")).toBeNull();
  });
});
