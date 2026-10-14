import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Markdown, OPEN_ARTIFACT_EVENT } from "./Markdown";

afterEach(cleanup);

// §34 (UX-016): [Title](artifact:path) renders as a chip that opens the artifact viewer via
// a window event; ordinary links keep the open-externally treatment.
describe("Markdown artifact links", () => {
  it("renders an artifact: link as a chip and dispatches the open event with the path", () => {
    const seen: string[] = [];
    const listener = (e: Event) => seen.push((e as CustomEvent).detail.path);
    window.addEventListener(OPEN_ARTIFACT_EVENT, listener);

    render(<Markdown text="Done — [Semiconductor dashboard](artifact:reports/semi.html)" />);
    const chip = screen.getByTestId("artifact-chip");
    expect(chip.textContent).toContain("Semiconductor dashboard");
    expect(chip.textContent).toContain("semi.html"); // filename shown under the title
    fireEvent.click(chip);
    expect(seen).toEqual(["reports/semi.html"]);

    window.removeEventListener(OPEN_ARTIFACT_EVENT, listener);
  });

  it("ordinary links stay external and never become chips", () => {
    const { container } = render(<Markdown text="see [the docs](https://example.com)" />);
    expect(screen.queryByTestId("artifact-chip")).toBeNull();
    const a = container.querySelector("a")!;
    expect(a.getAttribute("target")).toBe("_blank");
    expect(a.getAttribute("href")).toBe("https://example.com");
  });

  it("chip title falls back to the filename when the link text is empty", () => {
    vi.spyOn(window, "dispatchEvent");
    render(<Markdown text="[](artifact:out/report.pdf)" />);
    expect(screen.getByTestId("artifact-chip").textContent).toContain("report.pdf");
  });

  it("decodes a URI-encoded artifact href so the viewer can match the real file", () => {
    const seen: string[] = [];
    const listener = (e: Event) => seen.push((e as CustomEvent).detail.path);
    window.addEventListener(OPEN_ARTIFACT_EVENT, listener);

    const encoded = encodeURIComponent("sample-fixture.md");
    render(
      <Markdown text={`[sample-fixture.md](artifact:out/${encoded})`} />
    );
    const chip = screen.getByTestId("artifact-chip");
    expect(chip.textContent).toContain("sample-fixture.md");
    fireEvent.click(chip);
    expect(seen).toEqual([`out/sample-fixture.md`]);

    window.removeEventListener(OPEN_ARTIFACT_EVENT, listener);
  });

  it("keeps a literal percent that is not an escape sequence", () => {
    const seen: string[] = [];
    const listener = (e: Event) => seen.push((e as CustomEvent).detail.path);
    window.addEventListener(OPEN_ARTIFACT_EVENT, listener);

    render(<Markdown text="[](artifact:out/100%.md)" />);
    fireEvent.click(screen.getByTestId("artifact-chip"));
    expect(seen).toEqual(["out/100%.md"]);

    window.removeEventListener(OPEN_ARTIFACT_EVENT, listener);
  });
});

// Fenced code blocks carry a copy affordance (ChatGPT/Claude/WorkBuddy parity): the raw
// text must reach the clipboard unchanged, and inline code must not grow a button.
describe("Markdown code copy", () => {
  it("copies a fenced block's exact source", async () => {
    const written: string[] = [];
    const writeText = vi.fn((text: string) => {
      written.push(text);
      return Promise.resolve();
    });
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });

    render(<Markdown text={"```js\nconst a = 1;\nconsole.log(a);\n```"} />);
    const button = screen.getByTestId("md-code-copy");
    expect(button.textContent).toContain("Copy");
    fireEvent.click(button);
    // Verbatim source — react-markdown keeps the block's trailing newline, and a copy
    // that silently rewrites the user's code would be worse than no copy at all.
    expect(written).toEqual(["const a = 1;\nconsole.log(a);\n"]);
    // Confirms in place instead of leaving a dead button.
    await vi.waitFor(() => expect(screen.getByTestId("md-code-copy").textContent).toContain("Copied"));
  });

  it("leaves inline code alone", () => {
    const { container } = render(<Markdown text="call `run_shell` first" />);
    expect(screen.queryByTestId("md-code-copy")).toBeNull();
    expect(container.querySelectorAll(".md-code").length).toBe(0);
  });
});
