// The preview panel renders the common deliverables without leaving the app: Markdown, CSV and
// images (HTML and spreadsheets have their own tests). Rendering only reads the payload the
// runtime returns; nothing here writes a file.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { RightRail } from "./RightRail";
import { I18nProvider } from "@delta/i18n/I18nContext";
import type { JSX } from "react";

const { getArtifactsMock, readArtifactMock } = vi.hoisted(() => ({
  getArtifactsMock: vi.fn(),
  readArtifactMock: vi.fn(),
}));

vi.mock("../api", async (importOriginal) => {
  const original = await importOriginal<typeof import("../api")>();
  return {
    ...original,
    getArtifacts: (...a: unknown[]) => getArtifactsMock(...a),
    readArtifact: (...a: unknown[]) => readArtifactMock(...a),
  };
});

const props = () => ({
  active: true,
  sessionId: "s1",
  refreshKey: 0,
  toolNames: [],
  todo: [] as never[],
  running: false,
  showArtifacts: true,
});

const wrap = (el: JSX.Element) => <I18nProvider locale="en-US">{el}</I18nProvider>;

const open = async (name: string, content: Record<string, unknown>) => {
  getArtifactsMock.mockResolvedValue([{ path: name, name, abs_path: `/tmp/ws/${name}` }]);
  readArtifactMock.mockResolvedValue({ ok: true, path: name, ...content });
  render(wrap(<RightRail {...props()} />));
  fireEvent.click(await screen.findByRole("button", { name: new RegExp(name.replace(".", "\\."), "i") }));
};

describe("artifact preview", () => {
  afterEach(cleanup);
  beforeEach(() => {
    getArtifactsMock.mockReset();
    readArtifactMock.mockReset();
  });

  it("offers a reusable error and retry action after a failed artifact read", async () => {
    getArtifactsMock.mockResolvedValue([{ path: "broken.md", name: "broken.md", abs_path: "/tmp/ws/broken.md" }]);
    readArtifactMock.mockRejectedValueOnce(new Error("Read refused"))
      .mockResolvedValueOnce({ kind: "markdown", content: "# Recovered document" });
    render(wrap(<RightRail {...props()} />));
    fireEvent.click(await screen.findByRole("button", { name: /broken\.md/i }));
    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.getByText("Read refused")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Retry preview" }));
    expect(await screen.findByRole("heading", { name: "Recovered document" })).toBeTruthy();
    expect(readArtifactMock).toHaveBeenCalledTimes(2);
  });

  it("renders a Markdown file as formatted text", async () => {
    await open("notes.md", { kind: "markdown", content: "# Weekly plan\n\n- ship the report" });
    expect(await screen.findByRole("heading", { name: "Weekly plan" })).toBeTruthy();
    expect(screen.getByText("ship the report")).toBeTruthy();
  });

  it("uses the shared preview toolbar to refresh a Markdown deliverable", async () => {
    await open("notes.md", { kind: "markdown", content: "# First version" });
    expect(await screen.findByRole("heading", { name: "First version" })).toBeTruthy();
    const toolbar = screen.getByRole("toolbar", { name: "Preview actions" });
    expect(toolbar).toBeTruthy();
    readArtifactMock.mockResolvedValue({ kind: "markdown", content: "# Current version" });
    fireEvent.click(screen.getByRole("button", { name: "Reload preview" }));
    expect(await screen.findByRole("heading", { name: "Current version" })).toBeTruthy();
    expect(readArtifactMock).toHaveBeenCalledTimes(2);
  });

  it("renders a CSV file as a table with its header row", async () => {
    await open("data.csv", { kind: "csv", content: "item,qty\nWidget,3\nGadget,5" });
    expect(await screen.findByText("Widget")).toBeTruthy();
    expect(screen.getByText("qty")).toBeTruthy();
    expect(screen.getByText("Gadget")).toBeTruthy();
  });

  it("renders a PNG as an image from the runtime's data url", async () => {
    const dataUrl = "data:image/png;base64,iVBORw0KGgo=";
    await open("chart.png", { kind: "image", data_url: dataUrl });
    const image = (await screen.findByRole("img")) as HTMLImageElement;
    expect(image.getAttribute("src")).toBe(dataUrl);
  });
});
