import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { I18nProvider } from "@delta/i18n/I18nContext";
import type { ReactNode } from "react";
import type { ArtifactTrust } from "../artifactTrust";
import { ArtifactDeliveryReview } from "./ArtifactDeliveryReview";

const wrap = (child: ReactNode) => <I18nProvider locale="en-US">{child}</I18nProvider>;
afterEach(cleanup);
const openReview = () => fireEvent.click(screen.getByRole("button", { name: "Delivery evidence" }));

describe("delivery evidence review", () => {
  it("shows recorded validation and previous-version fact without claiming current approval", () => {
    const trust: ArtifactTrust = {
      sha256: "abcdef",
      previousVersion: ".delta/versions/run-9/report.md",
      checks: [{ name: "pages", isOk: true, detail: "" }],
      isValidationOk: true,
    };
    render(wrap(<ArtifactDeliveryReview preview="available" checksLookup="loaded" trust={trust} />));
    expect(screen.queryByTestId("artifact-review-preview")).toBeNull();
    openReview();
    expect(screen.getByTestId("artifact-review-preview").textContent).toBe("Preview loaded");
    expect(screen.getByTestId("artifact-review-validation").textContent).toContain("Recorded checks passed: 1/1");
    expect(screen.getByTestId("artifact-review-previous").textContent).toBe("Earlier version path recorded");
    expect(screen.getByTestId("artifact-review-backup-path").textContent).toBe(".delta/versions/run-9/report.md");
    expect(screen.getByTestId("artifact-review-disclaimer").textContent).toContain("not a fresh validation");
  });

  it("distinguishes failed historical checks, unavailable evidence and unrecorded checks", () => {
    const bad: ArtifactTrust = {
      checks: [{ name: "content", isOk: false, detail: "missing" }],
      isValidationOk: false,
    };
    const { rerender } = render(wrap(<ArtifactDeliveryReview preview="unavailable" checksLookup="loaded" trust={bad} />));
    openReview();
    expect(screen.getByTestId("artifact-review-preview").textContent).toBe("Unavailable");
    expect(screen.getByTestId("artifact-review-validation").textContent).toContain("Recorded checks failed: 1/1");
    rerender(wrap(<ArtifactDeliveryReview preview="available" checksLookup="loaded" trust={null} />));
    expect(screen.getByTestId("artifact-review-validation").textContent).toBe("Not recorded");
    expect(screen.getByTestId("artifact-review-previous").textContent).toBe("Not recorded");
    rerender(wrap(<ArtifactDeliveryReview preview="loading" checksLookup="unavailable" trust={null} />));
    expect(screen.getByTestId("artifact-review-preview").textContent).toBe("Checking…");
    expect(screen.getByTestId("artifact-review-validation").textContent).toBe("Unavailable");
    expect(screen.queryByTestId("artifact-review-backup-path")).toBeNull();
  });

  it("does not turn loading into missing or approved delivery evidence", () => {
    render(wrap(<ArtifactDeliveryReview preview="loading" checksLookup="loading" trust={null} />));
    openReview();
    expect(screen.getByTestId("artifact-review-validation").textContent).toBe("Checking…");
    expect(screen.getByTestId("artifact-review-previous").textContent).toBe("Checking…");
    expect(screen.queryByText(/approved for delivery/i)).toBeNull();
  });
});
