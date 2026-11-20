import { afterEach, describe, expect, it, vi } from "vitest";
import { createRef } from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { SegmentedControl } from "./SegmentedControl";
import { FieldInput } from "./FieldInput";
import { Button } from "./Button";

afterEach(cleanup);

const modes = [
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
  { value: "auto", label: "Automatic" },
] as const;

describe("shared form primitives", () => {
  it("exposes a labelled group, selection and stable button semantics", () => {
    const change = vi.fn();
    render(<SegmentedControl label="Theme" value="light" options={modes.map((mode) => ({ ...mode, testId: `mode-${mode.value}` }))} onChange={change} />);
    const group = screen.getByRole("group", { name: "Theme" });
    const light = screen.getByRole("button", { name: "Light" });
    const dark = screen.getByRole("button", { name: "Dark" });
    expect(group).toBeTruthy();
    expect(screen.getByTestId("mode-light")).toBe(light);
    expect(light.getAttribute("aria-pressed")).toBe("true");
    expect(dark.getAttribute("aria-pressed")).toBe("false");
    expect(light.tabIndex).toBe(0);
    expect(dark.tabIndex).toBe(-1);
    fireEvent.click(dark);
    expect(change).toHaveBeenCalledWith("dark");
  });

  it("moves focus and selects with arrow, Home and End keys", () => {
    const change = vi.fn();
    const { rerender } = render(
      <SegmentedControl label="Theme" value="light" options={modes} onChange={change} />,
    );
    const light = screen.getByRole("button", { name: "Light" });
    fireEvent.keyDown(light, { key: "ArrowRight" });
    expect(change).toHaveBeenLastCalledWith("dark");
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Dark" }));

    rerender(<SegmentedControl label="Theme" value="dark" options={modes} onChange={change} />);
    fireEvent.keyDown(screen.getByRole("button", { name: "Dark" }), { key: "End" });
    expect(change).toHaveBeenLastCalledWith("auto");

    rerender(<SegmentedControl label="Theme" value="auto" options={modes} onChange={change} />);
    fireEvent.keyDown(screen.getByRole("button", { name: "Automatic" }), { key: "Home" });
    expect(change).toHaveBeenLastCalledWith("light");
  });

  it("preserves native number constraints and change events", () => {
    const change = vi.fn();
    render(
      <label>
        Context budget
        <FieldInput type="number" min={0} max={2_000_000} data-testid="budget" className="w-28" onChange={change} />
      </label>,
    );
    const input = screen.getByRole("spinbutton", { name: "Context budget" }) as HTMLInputElement;
    expect(input.className).toContain("ds-field");
    expect(input.getAttribute("min")).toBe("0");
    expect(input.getAttribute("max")).toBe("2000000");
    fireEvent.change(input, { target: { value: "120000" } });
    expect(change).toHaveBeenCalledTimes(1);
    expect(input.value).toBe("120000");
  });
  it("reuses FieldInput for borderless inputs inside a shared focus surface", () => {
    render(<FieldInput appearance="embedded" aria-label="Search chats" placeholder="Search chats" />);
    const input = screen.getByRole("textbox", { name: "Search chats" });
    expect(input.className).toContain("ds-field-embedded");
    expect(input.className).not.toContain("ds-field ");
    expect(input.getAttribute("placeholder")).toBe("Search chats");
    expect(input.getAttribute("appearance")).toBeNull();
  });

  it("preserves focus and forwarded refs for embedded fields", () => {
    const ref = createRef<HTMLInputElement>();
    render(<FieldInput appearance="embedded" ref={ref} aria-label="Search session" />);
    const field = screen.getByRole("textbox", { name: "Search session" });
    expect(ref.current).toBe(field);
    ref.current?.focus();
    expect(document.activeElement).toBe(field);
    expect(field.className).toContain("ds-field-embedded");
  });

  it("uses one accessible action hierarchy and safe button defaults", () => {
    const approve = vi.fn();
    const deny = vi.fn();
    render(
      <>
        <Button variant="primary" disabled onClick={approve}>Approve</Button>
        <Button variant="ghost" onClick={deny}>Deny</Button>
        <Button size="sm" data-testid="small-action">Details</Button>
      </>,
    );
    const primary = screen.getByRole("button", { name: "Approve" });
    const ghost = screen.getByRole("button", { name: "Deny" });
    expect(primary.className).toContain("ds-button-primary");
    expect(primary.getAttribute("type")).toBe("button");
    expect(ghost.className).toContain("ds-button-ghost");
    expect(screen.getByTestId("small-action").className).toContain("ds-button-sm");
    fireEvent.click(primary);
    expect(approve).not.toHaveBeenCalled();
    fireEvent.click(ghost);
    expect(deny).toHaveBeenCalledOnce();
  });

});
