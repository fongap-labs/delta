import { describe, expect, it } from "vitest";
import { modelAfterSettings } from "./modelSync";

describe("modelAfterSettings", () => {
  it("takes a default set after boot when the composer had none (the blank-model case)", () => {
    expect(modelAfterSettings("", "", "alias:Air")).toBe("alias:Air");
  });

  it("follows a changed default while the composer still shows the previous one", () => {
    expect(modelAfterSettings("alias:Air", "alias:Air", "alias:Max")).toBe("alias:Max");
  });

  it("keeps a model the user picked for this session over the settings default", () => {
    expect(modelAfterSettings("alias:Code-Max", "alias:Air", "alias:Max")).toBe("alias:Code-Max");
  });

  it("never replaces the composer model with an empty default", () => {
    expect(modelAfterSettings("alias:Air", "alias:Air", "")).toBe("alias:Air");
  });
});
