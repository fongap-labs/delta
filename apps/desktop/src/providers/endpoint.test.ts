import { describe, expect, it } from "vitest";
import { endpointPreview, resolveEndpoint } from "./endpoint";

describe("resolveEndpoint", () => {
  it("adds /v1 to a bare host", () => {
    expect(resolveEndpoint("https://api.example.test", "messages")).toBe(
      "https://api.example.test/v1/messages",
    );
  });

  it("does not duplicate /v1 when the base already carries it", () => {
    expect(resolveEndpoint("https://api.example.test/v1", "messages")).toBe(
      "https://api.example.test/v1/messages",
    );
    expect(resolveEndpoint("https://api.example.test/v1/", "responses")).toBe(
      "https://api.example.test/v1/responses",
    );
  });

  it("leaves a base URL that already names the endpoint alone", () => {
    expect(resolveEndpoint("https://gw.example.test/v1/messages", "messages")).toBe(
      "https://gw.example.test/v1/messages",
    );
  });

  it("keeps a non-v1 version segment as given", () => {
    expect(resolveEndpoint("https://gw.example.test/api/v2", "messages")).toBe(
      "https://gw.example.test/api/v2/messages",
    );
  });

  it("resolves nothing while the field is empty", () => {
    expect(resolveEndpoint("   ", "messages")).toBe("");
  });
});

describe("endpointPreview", () => {
  it("names the path each protocol is served at", () => {
    expect(endpointPreview("https://api.example.test", "openai_chat")).toBe(
      "https://api.example.test/v1/chat/completions",
    );
    expect(endpointPreview("https://api.anthropic.com", "anthropic")).toBe(
      "https://api.anthropic.com/v1/messages",
    );
  });
});
