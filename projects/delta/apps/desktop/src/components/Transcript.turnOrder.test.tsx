// A turn reads thinking → steps → answer; its step count keeps going across a question answered
// mid-turn; a step's dot follows its recorded outcome (a failed call is red, a finished one green).
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Transcript } from "./Transcript";
import { I18nProvider } from "@delta/i18n/I18nContext";
import type { Item } from "../types";

afterEach(cleanup);

const renderTranscript = (ui: React.ReactElement) =>
  render(<I18nProvider locale="en-US">{ui}</I18nProvider>);

describe("Transcript turn order and progress", () => {
  it("keeps counting steps across a question answered mid-turn", () => {
    const items: Item[] = [
      { kind: "user", text: "what happened yesterday?" },
      { kind: "tool", id: "t1", name: "web_search", args: { query: "a" }, status: "ok" },
      { kind: "question", question: "Which day?", resolved: "2026-10-15" },
      { kind: "tool", id: "t2", name: "web_search", args: { query: "b" }, status: "ok" },
      { kind: "tool", id: "t3", name: "web_fetch", args: { url: "https://example.test/news" }, status: "ok" },
    ];
    renderTranscript(<Transcript items={items} onApprove={vi.fn()} />);
    expect(screen.getByText("1 step")).toBeTruthy();
    expect(screen.getByText("3 steps")).toBeTruthy();
  });

  it("shows a turn's thinking above its steps, once, rather than inside the answer below them", () => {
    const items: Item[] = [
      { kind: "user", text: "find it" },
      { kind: "assistant", text: "", reasoning: "Search first." },
      { kind: "tool", id: "t1", name: "web_search", args: { query: "q" }, status: "ok" },
      { kind: "assistant", text: "Here is the answer.", reasoning: "Summarise the hits." },
    ];
    const { container } = renderTranscript(<Transcript items={items} onApprove={vi.fn()} />);
    const thinking = container.querySelectorAll(".thinking");
    const group = container.querySelector(".stepgroup")!;
    expect(thinking).toHaveLength(1);
    expect(thinking[0].compareDocumentPosition(group) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.getByText("Here is the answer.")).toBeTruthy();
  });

  it("shows the live thinking above a running turn's steps", () => {
    const items: Item[] = [
      { kind: "user", text: "go" },
      { kind: "tool", id: "t1", name: "grep", args: { pattern: "TODO" }, status: "…" },
    ];
    const { container } = renderTranscript(
      <Transcript items={items} running liveReasoning="Checking the repo" onApprove={vi.fn()} />,
    );
    const thinking = container.querySelector(".thinking")!;
    const group = container.querySelector(".stepgroup")!;
    expect(thinking.compareDocumentPosition(group) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("colours a step's dot from its recorded outcome", () => {
    const items: Item[] = [
      { kind: "user", text: "fetch both" },
      { kind: "tool", id: "t1", name: "web_fetch", args: { url: "https://a.test" }, status: "ok" },
      { kind: "tool", id: "t2", name: "web_fetch", args: { url: "https://b.test" }, status: "error" },
    ];
    const { container } = renderTranscript(<Transcript items={items} onApprove={vi.fn()} />);
    fireEvent.click(container.querySelector("summary.stepgroup-head")!);
    const [finished, failed] = screen.getAllByTestId("turn-step").map((row) => row.firstElementChild!.className);
    expect(finished).toContain("text-ok");
    expect(failed).toContain("text-danger");
  });
});
