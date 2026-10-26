import type { ReactElement } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { I18nProvider } from "@delta/i18n/I18nContext";
import { StatisticsView } from "./StatisticsView";

const listStatisticsTools = vi.fn();
const runStatisticsTool = vi.fn();
vi.mock("../api", () => ({
  listStatisticsTools: () => listStatisticsTools(),
  runStatisticsTool: (...args: unknown[]) => runStatisticsTool(...args),
}));

const f2Tool = {
  name: "statistics_f2",
  description: "Similarity factor f2 between two dissolution profiles.",
  parameters: {
    type: "object",
    required: ["path", "reference", "test"],
    properties: {
      path: { type: "string", description: "CSV file path, relative to the workspace." },
      reference: { type: "string", description: "Reference column." },
      test: { type: "string", description: "Test column." },
    },
  },
};

const wrap = (el: ReactElement) => <I18nProvider locale="en-US">{el}</I18nProvider>;

describe("StatisticsView", () => {
  afterEach(() => {
    cleanup();
    listStatisticsTools.mockReset();
    runStatisticsTool.mockReset();
  });

  it("runs a tool with its filled arguments in the open workspace and shows the result", async () => {
    listStatisticsTools.mockResolvedValue([f2Tool]);
    runStatisticsTool.mockResolvedValue({ f2: 73.02046884880939, points: 3 });
    render(wrap(<StatisticsView workspace="/work/a" />));

    await screen.findByText("statistics_f2");
    const run = screen.getByRole("button", { name: "Run" }) as HTMLButtonElement;
    expect(run.disabled).toBe(true);

    const inputs = screen.getAllByRole("textbox");
    fireEvent.change(inputs[0], { target: { value: "profiles.csv" } });
    fireEvent.change(inputs[1], { target: { value: "reference" } });
    fireEvent.change(inputs[2], { target: { value: "test" } });
    expect((screen.getByRole("button", { name: "Run" }) as HTMLButtonElement).disabled).toBe(false);

    fireEvent.click(screen.getByRole("button", { name: "Run" }));
    await waitFor(() => expect(screen.getByTestId("statistics-result")).toBeTruthy());
    expect(runStatisticsTool).toHaveBeenCalledWith(
      "statistics_f2",
      { path: "profiles.csv", reference: "reference", test: "test" },
      "/work/a",
    );
    expect(screen.getByText("73.020469")).toBeTruthy();
    expect(screen.getByText("3")).toBeTruthy();
  });

  it("shows the error when the calculation does not run", async () => {
    listStatisticsTools.mockResolvedValue([f2Tool]);
    runStatisticsTool.mockResolvedValue({ ok: false, error: "column not found: test" });
    render(wrap(<StatisticsView workspace="/work/a" />));

    await screen.findByText("statistics_f2");
    for (const input of screen.getAllByRole("textbox")) {
      fireEvent.change(input, { target: { value: "x" } });
    }
    fireEvent.click(screen.getByRole("button", { name: "Run" }));
    expect(await screen.findByText(/column not found: test/)).toBeTruthy();
  });

  it("says no local statistics engine is available when the list is empty", async () => {
    listStatisticsTools.mockResolvedValue([]);
    render(wrap(<StatisticsView workspace="/work/a" />));

    expect(await screen.findByText(/No local statistics engine is available/)).toBeTruthy();
  });

  it("asks for a workspace and cannot run without one", async () => {
    listStatisticsTools.mockResolvedValue([f2Tool]);
    render(wrap(<StatisticsView />));

    await screen.findByText("statistics_f2");
    expect(screen.getByText(/Open a conversation in a workspace first/)).toBeTruthy();
    for (const input of screen.getAllByRole("textbox")) {
      fireEvent.change(input, { target: { value: "x" } });
    }
    expect((screen.getByRole("button", { name: "Run" }) as HTMLButtonElement).disabled).toBe(true);
    expect(runStatisticsTool).not.toHaveBeenCalled();
  });
});
