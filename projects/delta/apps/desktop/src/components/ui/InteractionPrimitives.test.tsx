import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { IconButton } from "./IconButton";
import { DisclosureSection } from "./DisclosureSection";
import { DialogSurface } from "./DialogSurface";

afterEach(cleanup);

describe("shared interaction primitives", () => {
  it("gives icon-only actions an accessible name", () => {
    const onClick = vi.fn();
    render(<IconButton icon="refresh" label="Refresh artifacts" onClick={onClick} />);
    const button = screen.getByRole("button", { name: "Refresh artifacts" });
    expect(button.getAttribute("type")).toBe("button");
    fireEvent.click(button);
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it("exposes disclosure state and the labelled region", () => {
    const onToggle = vi.fn();
    const { rerender } = render(
      <DisclosureSection title="Progress" isOpen={false} onToggle={onToggle}>
        <span>Work in progress</span>
      </DisclosureSection>,
    );
    const trigger = screen.getByRole("button", { name: "Progress" });
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByRole("region", { name: "Progress" })).toBeNull();
    fireEvent.click(trigger);
    expect(onToggle).toHaveBeenCalledOnce();
    rerender(
      <DisclosureSection title="Progress" isOpen onToggle={onToggle}>
        <span>Work in progress</span>
      </DisclosureSection>,
    );
    const region = screen.getByRole("region", { name: "Progress" });
    expect(trigger.getAttribute("aria-expanded")).toBe("true");
    expect(trigger.getAttribute("aria-controls")).toBe(region.id);
    expect(screen.getByText("Work in progress")).toBeTruthy();
  });

  it("renders a labelled dialog without taking over dismissal", () => {
    render(<DialogSurface label="Search chats" tabIndex={-1}>Search content</DialogSurface>);
    const dialog = screen.getByRole("dialog", { name: "Search chats" });
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(dialog.className).toContain("ds-dialog-surface");
    expect(screen.getByText("Search content")).toBeTruthy();
  });
});
