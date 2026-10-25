// The preview panel renders the common deliverables without leaving the app: Markdown, CSV and
// images (HTML and spreadsheets have their own tests). Rendering only reads the payload the
// runtime returns; nothing here writes a file.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
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
  beforeEach(() => {
    getArtifactsMock.mockReset();
    readArtifactMock.mockReset();
  });

  it("renders a Markdown file as formatted text", async () => {
    await open("notes.md", { kind: "markdown", content: "# Weekly plan\n\n- ship the report" });
    expect(await screen.findByRole("heading", { name: "Weekly plan" })).toBeTruthy();
    expect(screen.getByText("ship the report")).toBeTruthy();
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
