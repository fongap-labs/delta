import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import type { Item } from "../types";
import { I18nProvider } from "@delta/i18n/I18nContext";
import { ActivitySummary } from "./ActivitySummary";

const wrap = (view: ReactNode) => <I18nProvider locale="en-US">{view}</I18nProvider>;
afterEach(cleanup);

describe("ActivitySummary", () => {
  it("shows recorded outcomes, longest measured call and reported token totals", () => {
    const items: Item[] = [
      { kind: "user", text: "Research" },
      { kind: "tool", id: "a", name: "read_file", args: {}, status: "ok", startedAtMs: 1_800_000_000_000, finishedAtMs: 1_800_000_002_500 },
      { kind: "tool", id: "b", name: "web_fetch", args: {}, status: "error", startedAtMs: 1_800_000_001_000, finishedAtMs: 1_800_000_002_000 },
      { kind: "tool", id: "c", name: "search", args: {}, status: "…" },
    ];
    render(wrap(<ActivitySummary items={items} usage={{ byModel: { model: { model: "model", input: 100, output: 50, cache_read: 20, cache_write: 10 } }, context: 130 }} />));
    expect(screen.getByTestId("rail-activity-total").textContent).toBe("3");
    expect(screen.getByTestId("rail-activity-status").textContent).toContain("Completed 1");
    expect(screen.getByTestId("rail-activity-status").textContent).toContain("Failed 1");
    expect(screen.getByTestId("rail-activity-status").textContent).toContain("Running 1");
    expect(screen.getByTestId("rail-activity-longest").textContent).toBe("2.5 s");
    expect(screen.getByTestId("rail-activity-partial").textContent).toContain("2/3");
    expect(screen.getByTestId("rail-activity-tokens").textContent).toBe("180");
  });

  it("labels missing usage separately from a real reported zero", () => {
    const items: Item[] = [{ kind: "tool", id: "old", name: "search", args: {}, status: "unknown" }];
    const { rerender } = render(wrap(<ActivitySummary items={items} />));
    expect(screen.getByTestId("rail-activity-tokens").textContent).toBe("Not reported");
    expect(screen.queryByTestId("rail-activity-longest")).toBeNull();
    rerender(wrap(<ActivitySummary items={items} usage={{ byModel: { unknown: { model: null, input: 0, output: 0, cache_read: 0, cache_write: 0 } }, context: 0 }} />));
    expect(screen.getByTestId("rail-activity-tokens").textContent).toBe("0");
  });

  it("omits itself when neither tool activity nor reported tokens exist", () => {
    render(wrap(<ActivitySummary items={[{ kind: "user", text: "Hello" }]} usage={{ byModel: {}, context: 0 }} />));
    expect(screen.queryByTestId("rail-activity-summary")).toBeNull();
  });
});
