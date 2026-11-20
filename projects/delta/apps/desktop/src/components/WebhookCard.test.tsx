import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@delta/i18n/I18nContext";
import { WebhookCard } from "./WebhookCard";
import {
  getWebhookSettings, saveWebhookSettings, testWebhook,
  type WebhookSettings,
} from "../api";

vi.mock("../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../api")>()),
  getWebhookSettings: vi.fn(),
  saveWebhookSettings: vi.fn(),
  testWebhook: vi.fn(),
}));

const OFF: WebhookSettings = { url: null, is_enabled: false, include_details: false };
const view = () => render(<I18nProvider locale="en-US"><WebhookCard /></I18nProvider>);

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(getWebhookSettings).mockResolvedValue(OFF);
  vi.mocked(saveWebhookSettings).mockImplementation(async (value) => value);
  vi.mocked(testWebhook).mockResolvedValue({
    result: "skipped", host: "hooks.example", reason: "restricted by privacy policy",
  });
});
afterEach(cleanup);

describe("completion webhook shared controls", () => {
  it("starts disabled and never posts on load or on edits alone", async () => {
    view();
    const save = screen.getByTestId("webhook-save") as HTMLButtonElement;
    const test = screen.getByTestId("webhook-test") as HTMLButtonElement;
    const url = screen.getByTestId("webhook-url") as HTMLInputElement;
    await waitFor(() => expect(save.disabled).toBe(true));
    expect(url.className).toContain("ds-field");
    expect(url.getAttribute("aria-label")).toBeTruthy();
    expect(save.className).toContain("ds-button-primary");
    expect(save.getAttribute("type")).toBe("button");
    expect(test.disabled).toBe(true);
    expect((screen.getByTestId("webhook-enabled") as HTMLInputElement).checked).toBe(false);
    expect((screen.getByTestId("webhook-details") as HTMLInputElement).checked).toBe(false);
    fireEvent.change(url, { target: { value: " https://hooks.example/delta " } });
    expect(save.disabled).toBe(false);
    expect(test.disabled).toBe(true);
    expect(saveWebhookSettings).not.toHaveBeenCalled();
    expect(testWebhook).not.toHaveBeenCalled();
  });

  it("preserves explicit save and test actions and reports a privacy-blocked test without claiming delivery", async () => {
    view();
    const save = screen.getByTestId("webhook-save");
    const test = screen.getByTestId("webhook-test") as HTMLButtonElement;
    await waitFor(() => expect((save as HTMLButtonElement).disabled).toBe(true));
    fireEvent.change(screen.getByTestId("webhook-url"), { target: { value: " https://hooks.example/delta " } });
    fireEvent.click(screen.getByTestId("webhook-enabled"));
    expect(test.disabled).toBe(true);
    fireEvent.click(save);
    await waitFor(() => expect(saveWebhookSettings).toHaveBeenCalledWith({
      url: "https://hooks.example/delta", is_enabled: true, include_details: false,
    }));
    await waitFor(() => expect(test.disabled).toBe(false));
    expect(testWebhook).not.toHaveBeenCalled();
    expect(test.className).toContain("ds-button-sm");
    expect(test.getAttribute("aria-busy")).toBe("false");
    fireEvent.click(test);
    await waitFor(() => expect(testWebhook).toHaveBeenCalledTimes(1));
    expect((await screen.findByTestId("webhook-message")).textContent).toContain("Not sent");
  });
});
