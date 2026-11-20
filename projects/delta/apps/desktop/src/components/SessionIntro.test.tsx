import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { I18nProvider } from "@delta/i18n/I18nContext";
import { SessionIntro } from "./SessionIntro";

const renderIntro = (onPick?: (value: string) => void) =>
  render(<I18nProvider locale="en-US"><SessionIntro onPick={onPick} /></I18nProvider>);

afterEach(cleanup);

describe("SessionIntro guided actions", () => {
  it("uses shared accessible tabs and links the selected kind to its examples", () => {
    const onPick = vi.fn();
    renderIntro(onPick);
    const office = screen.getByRole("tab", { name: "Office" });
    const research = screen.getByRole("tab", { name: "Research" });
    const panel = screen.getByRole("tabpanel", { name: "Office" });
    expect(office.className).toContain("ds-button");
    expect(office.getAttribute("type")).toBe("button");
    expect(office.getAttribute("aria-selected")).toBe("true");
    expect(office.getAttribute("tabindex")).toBe("0");
    expect(research.getAttribute("tabindex")).toBe("-1");
    expect(office.getAttribute("aria-controls")).toBe(panel.id);
    expect(within(panel).getAllByTestId("intro-example")).toHaveLength(3);
    expect(onPick).not.toHaveBeenCalled();
  });

  it("supports Arrow, Home and End keys with roving focus and selection", () => {
    const onPick = vi.fn();
    renderIntro(onPick);
    const office = screen.getByRole("tab", { name: "Office" });
    const research = screen.getByRole("tab", { name: "Research" });
    const create = screen.getByRole("tab", { name: "Create" });
    office.focus();
    fireEvent.keyDown(office, { key: "ArrowRight" });
    expect(document.activeElement).toBe(research);
    expect(research.getAttribute("aria-selected")).toBe("true");
    expect(screen.getByRole("tabpanel", { name: "Research" })).toBeTruthy();
    fireEvent.keyDown(research, { key: "End" });
    expect(document.activeElement).toBe(create);
    expect(create.getAttribute("tabindex")).toBe("0");
    fireEvent.keyDown(create, { key: "ArrowRight" });
    expect(document.activeElement).toBe(office);
    fireEvent.keyDown(office, { key: "Home" });
    expect(document.activeElement).toBe(office);
    fireEvent.keyDown(office, { key: "ArrowLeft" });
    expect(document.activeElement).toBe(create);
    expect(onPick).not.toHaveBeenCalled();
  });

  it("only invokes the composer callback after an explicit example click", () => {
    const onPick = vi.fn();
    renderIntro(onPick);
    fireEvent.click(screen.getByRole("tab", { name: "Research" }));
    expect(onPick).not.toHaveBeenCalled();
    const panel = screen.getByRole("tabpanel", { name: "Research" });
    const example = within(panel).getByRole("button", {
      name: "Read this PDF and give me a one-page summary",
    });
    expect(example.className).toContain("ds-button");
    expect(example.getAttribute("type")).toBe("button");
    fireEvent.click(example);
    expect(onPick).toHaveBeenCalledTimes(1);
    expect(onPick).toHaveBeenCalledWith("Read this PDF and give me a one-page summary");
  });

  it("hides guided actions when no composer callback is provided", () => {
    renderIntro();
    expect(screen.getByText("How can I help you?")).toBeTruthy();
    expect(screen.queryByRole("tablist")).toBeNull();
    expect(screen.queryByRole("tabpanel")).toBeNull();
    expect(screen.queryByTestId("intro-example")).toBeNull();
  });
});
