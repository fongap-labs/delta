import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { DisclosureTrigger, DisclosureContent } from "./DisclosureControl";

afterEach(cleanup);

describe("transcript disclosure controls", () => {
  it("announces closed state without mounting hidden content", () => {
    render(
      <>
        <DisclosureTrigger isOpen={false} contentId="raw-details">raw</DisclosureTrigger>
        <DisclosureContent isOpen={false} id="raw-details" label="Tool raw output">
          Hidden output
        </DisclosureContent>
      </>,
    );
    const trigger = screen.getByRole("button", { name: "raw" });
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
    expect(trigger.getAttribute("aria-controls")).toBe("raw-details");
    expect(trigger.getAttribute("type")).toBe("button");
    expect(screen.queryByText("Hidden output")).toBeNull();
  });

  it("links an expanded button to an accessible region and retains callbacks", () => {
    const onClick = vi.fn();
    render(
      <>
        <DisclosureTrigger isOpen contentId="thinking-body" onClick={onClick}>Thinking</DisclosureTrigger>
        <DisclosureContent isOpen id="thinking-body" label="Thinking" testId="thinking-test">
          Reasoning text
        </DisclosureContent>
      </>,
    );
    const trigger = screen.getByRole("button", { name: "Thinking" });
    const region = screen.getByRole("region", { name: "Thinking" });
    expect(trigger.getAttribute("aria-expanded")).toBe("true");
    expect(trigger.getAttribute("aria-controls")).toBe(region.id);
    expect(screen.getByTestId("thinking-test").textContent).toBe("Reasoning text");
    fireEvent.click(trigger);
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("preserves the pre element for raw tool results", () => {
    render(<DisclosureContent isOpen id="tool-raw" label="Raw tool result" as="pre">hello</DisclosureContent>);
    expect(screen.getByRole("region", { name: "Raw tool result" }).tagName).toBe("PRE");
  });
});
