import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Composer } from "./Composer";
import { I18nProvider } from "@delta/i18n/I18nContext";

const baseProps = (extra: Partial<Parameters<typeof Composer>[0]> = {}) => ({
  mode: "interactive",
  model: "fong:code-max",
  models: ["fong:code-max"],
  running: false,
  connected: true,
  onSend: vi.fn(),
  onInterrupt: vi.fn(),
  onModeChange: vi.fn(),
  onModelChange: vi.fn(),
  ...extra,
});
const renderComposer = (extra: Partial<Parameters<typeof Composer>[0]> = {}) =>
  render(
    <I18nProvider locale="en-US">
      <Composer {...baseProps(extra)} />
    </I18nProvider>,
  );

afterEach(cleanup);

describe("composer runtime controls", () => {
  it("shows the active reasoning effort beside the model and updates the session setting", () => {
    const onReasoningEffortChange = vi.fn();
    renderComposer({ reasoningEffort: "low", onReasoningEffortChange });
    const trigger = screen.getByTestId("reasoning-menu-trigger");
    expect(trigger.textContent).toContain("Light");
    fireEvent.click(trigger);
    fireEvent.click(screen.getByRole("menuitemradio", { name: /Deep/ }));
    expect(onReasoningEffortChange).toHaveBeenCalledWith("high");
  });

  it("keeps one active composer for steering while preserving stop and disabled reasoning", () => {
    const onSend = vi.fn();
    const onInterrupt = vi.fn();
    renderComposer({
      running: true,
      reasoningEffort: "low",
      onReasoningEffortChange: vi.fn(),
      onSend,
      onInterrupt,
    });

    const reasoning = screen.getByTestId("reasoning-menu-trigger");
    expect(reasoning.textContent).toContain("Light");
    expect(reasoning.hasAttribute("disabled")).toBe(true);

    const input = screen.getByLabelText("Message for the current run");
    fireEvent.change(input, { target: { value: "use the shorter path" } });
    fireEvent.keyDown(input, { key: "Enter", code: "Enter" });
    expect(onSend).toHaveBeenCalledWith("use the shorter path");

    fireEvent.click(screen.getByRole("button", { name: /Stop/ }));
    expect(onInterrupt).toHaveBeenCalledTimes(1);
  });
});

describe("composer during a run", () => {
  it("keeps the run's model visible", () => {
    renderComposer({ running: true, model: "fong:code-max", modelLabels: { "fong:code-max": "Code Max" } });
    expect(screen.getByTestId("running-model").textContent).toBe("Code Max");
  });

  it("answers Stop at once: the button reads Stopping… and cannot be pressed twice", () => {
    renderComposer({ running: true, isStopping: true });
    const stop = screen.getByTestId("composer-stop") as HTMLButtonElement;
    expect(stop.textContent).toContain("Stopping");
    expect(stop.disabled).toBe(true);
  });
});

describe("messages queued during a run", () => {
  const queue = [
    { id: "q1", text: "first queued" },
    { id: "q2", text: "second queued" },
  ];

  it("queues a typed message instead of interjecting it", () => {
    const onSend = vi.fn();
    const onQueue = vi.fn();
    renderComposer({ running: true, onSend, onQueue });
    const input = screen.getByLabelText("Message for the current run");
    fireEvent.change(input, { target: { value: "  next step  " } });
    fireEvent.keyDown(input, { key: "Enter", code: "Enter" });
    expect(onQueue).toHaveBeenCalledWith("next step");
    expect(onSend).not.toHaveBeenCalled();
    expect((input as HTMLTextAreaElement).value).toBe("");
  });

  it("interjects every queued message, then the draft, on Cmd/Ctrl+Enter", () => {
    const onQueueInterject = vi.fn();
    renderComposer({ running: true, queue, onQueue: vi.fn(), onQueueInterject });
    const input = screen.getByLabelText("Message for the current run");
    expect(input.getAttribute("placeholder")).toContain("Cmd/Ctrl+Enter");
    fireEvent.change(input, { target: { value: "and this" } });
    fireEvent.keyDown(input, { key: "Enter", code: "Enter", ctrlKey: true });
    expect(onQueueInterject).toHaveBeenCalledWith(["q1", "q2"], "and this");
    fireEvent.keyDown(input, { key: "Enter", code: "Enter", metaKey: true });
    expect(onQueueInterject).toHaveBeenLastCalledWith(["q1", "q2"], undefined);
  });

  it("does nothing on Cmd/Ctrl+Enter when there is nothing queued or typed", () => {
    const onQueueInterject = vi.fn();
    renderComposer({ running: true, onQueue: vi.fn(), onQueueInterject });
    fireEvent.keyDown(screen.getByLabelText("Message for the current run"), { key: "Enter", ctrlKey: true });
    expect(onQueueInterject).not.toHaveBeenCalled();
  });

  it("lists the queue and lets a row be changed, dropped or sent ahead", () => {
    const onQueueRemove = vi.fn();
    const onQueueInterject = vi.fn();
    renderComposer({ running: true, queue, onQueue: vi.fn(), onQueueRemove, onQueueInterject });
    const rows = screen.getAllByTestId("queued-message");
    expect(rows.map((row) => row.textContent)).toEqual(["first queued", "second queued"]);

    fireEvent.click(screen.getAllByTestId("queued-remove")[1]);
    expect(onQueueRemove).toHaveBeenLastCalledWith("q2");

    fireEvent.click(screen.getAllByTestId("queued-interject")[0]);
    expect(onQueueInterject).toHaveBeenCalledWith(["q1"]);

    // Editing takes the row back into the box, ahead of any draft already there.
    const input = screen.getByLabelText("Message for the current run") as HTMLTextAreaElement;
    fireEvent.change(input, { target: { value: "draft" } });
    fireEvent.click(screen.getAllByTestId("queued-edit")[0]);
    expect(onQueueRemove).toHaveBeenLastCalledWith("q1");
    expect(input.value).toBe("first queued\ndraft");
  });

  it("keeps a queue left behind visible when no run is going, and sends a row on request", () => {
    const onQueueInterject = vi.fn();
    renderComposer({ running: false, queue, onQueueInterject });
    expect(screen.getAllByTestId("queued-message")).toHaveLength(2);
    const send = screen.getAllByTestId("queued-interject")[0];
    expect(send.getAttribute("aria-label")).toBe("Send now");
    fireEvent.click(send);
    expect(onQueueInterject).toHaveBeenCalledWith(["q1"]);
  });

  it("shows no queue when nothing is queued", () => {
    renderComposer({ running: true, onQueue: vi.fn() });
    expect(screen.queryByTestId("queued-messages")).toBeNull();
  });
});

describe("composer attachment entry points", () => {
  it("labels the constrained picker honestly", () => {
    renderComposer();
    fireEvent.click(screen.getByLabelText("Attach files"));
    expect(screen.getByText("Text and code")).toBeTruthy();
    expect(screen.getByText("Word, Excel or PowerPoint")).toBeTruthy();
    expect(screen.getByText(/Archives and other binary files are not supported/)).toBeTruthy();
  });

  it("picker, drop, and paste show the same unsupported-format rejection", async () => {
    const { container } = renderComposer();
    const zip = new File(["binary"], "archive.zip", { type: "application/zip" });
    const input = container.querySelector('input[type="file"]') as HTMLInputElement;

    fireEvent.change(input, { target: { files: [zip] } });
    await screen.findByText(/archive\.zip: Unsupported format/);

    fireEvent.drop(container.querySelector(".composer")!, {
      dataTransfer: { files: [zip] },
    });
    await waitFor(() => expect(screen.getByTestId("attach-notice").textContent).toContain("archive.zip: Unsupported format"));

    fireEvent.paste(container.querySelector("textarea")!, {
      clipboardData: {
        items: [{ kind: "file", type: "application/zip", getAsFile: () => zip }],
      },
    });
    await waitFor(() => expect(screen.getByTestId("attach-notice").textContent).toContain("archive.zip: Unsupported format"));
  });
});
