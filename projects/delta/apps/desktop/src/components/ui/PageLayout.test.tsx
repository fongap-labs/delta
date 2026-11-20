import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Dropdown } from "../Dropdown";
import { SelectMenu } from "../SelectMenu";
import { PageLayout } from "./PageLayout";

afterEach(cleanup);

describe("Delta design-system navigation", () => {
  it("keeps semantic page navigation and selection", () => {
    const onChange = vi.fn();
    render(
      <PageLayout
        title="Settings"
        icon="gear"
        tabs={[
          { key: "general", label: "General", icon: "gear" },
          { key: "models", label: "Models", icon: "code" },
        ]}
        active="general"
        onChange={onChange}
      >
        <div>Page content</div>
      </PageLayout>,
    );
    expect(screen.getByRole("navigation", { name: "Settings" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "General" }).getAttribute("aria-current")).toBe("page");
    fireEvent.click(screen.getByRole("button", { name: "Models" }));
    expect(onChange).toHaveBeenCalledWith("models");
    expect(screen.getByText("Page content")).toBeTruthy();
  });
});

describe("Delta option controls", () => {
  it("supports keyboard selection and restores focus in model dropdown", () => {
    const onChange = vi.fn();
    render(
      <Dropdown
        prefix="Model"
        value="fast"
        onChange={onChange}
        options={[{ value: "fast", label: "Fast" }, { value: "deep", label: "Deep" }]}
      />,
    );
    const trigger = screen.getByRole("button", { name: "Model: Fast" });
    fireEvent.keyDown(trigger, { key: "ArrowDown" });
    expect(screen.getByRole("menu")).toBeTruthy();
    const first = screen.getByRole("menuitemradio", { name: "Fast" });
    expect(document.activeElement).toBe(first);
    fireEvent.keyDown(first, { key: "ArrowDown" });
    const second = screen.getByRole("menuitemradio", { name: "Deep" });
    expect(document.activeElement).toBe(second);
    fireEvent.click(second);
    expect(onChange).toHaveBeenCalledWith("deep");
    expect(document.activeElement).toBe(trigger);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("retains grouped select content and closes on Escape", () => {
    const onChange = vi.fn();
    render(
      <SelectMenu
        ariaLabel="Provider"
        value="a"
        onChange={onChange}
        options={[
          { value: "a", label: "Provider A", group: "Ready", dot: true },
          { value: "b", label: "Provider B", sub: "Not configured" },
        ]}
      />,
    );
    const trigger = screen.getByRole("button", { name: "Provider" });
    fireEvent.click(trigger);
    expect(screen.getByRole("listbox", { name: "Provider" })).toBeTruthy();
    expect(screen.getByText("Ready")).toBeTruthy();
    const first = screen.getByRole("option", { name: "Provider A" });
    fireEvent.keyDown(first, { key: "End" });
    expect(document.activeElement).toBe(screen.getByRole("option", { name: /Provider B/ }));
    fireEvent.keyDown(document.activeElement as HTMLElement, { key: "Escape" });
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(document.activeElement).toBe(trigger);
    expect(onChange).not.toHaveBeenCalled();
  });
});
