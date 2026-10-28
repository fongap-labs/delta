import { describe, expect, it } from "vitest";
import { thinkingSummary } from "./components/Transcript";

describe("thinkingSummary", () => {
  it("uses the first non-empty line, flattened, without markdown markers", () => {
    expect(thinkingSummary("\n\n## Plan the   digest\nthen write")).toBe("Plan the digest");
    expect(thinkingSummary("- weigh the options")).toBe("weigh the options");
  });
  it("clips long lines with an ellipsis and tolerates empty input", () => {
    const out = thinkingSummary("x".repeat(200), 20);
    expect(out.length).toBe(20);
    expect(out.endsWith("…")).toBe(true);
    expect(thinkingSummary("   \n  ")).toBe("");
  });
});
