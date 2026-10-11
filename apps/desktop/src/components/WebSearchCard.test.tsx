import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { I18nProvider } from "@delta/i18n/I18nContext";

const api = vi.hoisted(() => ({ get: vi.fn(), set: vi.fn() }));
vi.mock("../api", async (original) => ({
  ...(await original<typeof import("../api")>()),
  getWebSearchSettings: api.get,
  setWebSearchSettings: api.set,
}));

import { WebSearchCard } from "./SettingsView";

const view = () =>
  render(
    <I18nProvider locale="en-US">
      <WebSearchCard />
    </I18nProvider>,
  );
const choose = (name: string) => fireEvent.click(screen.getByRole("button", { name }));

beforeEach(() => {
  api.get.mockReset().mockResolvedValue({ provider: "auto", has_key: false });
  api.set.mockReset().mockResolvedValue({ ok: true, provider: "brave", has_key: true });
});
afterEach(cleanup);

describe("web search settings", () => {
  it("starts on the free search with nothing to fill in and nothing to save", async () => {
    view();
    await screen.findByTestId("web-search-card");
    expect(screen.queryByTestId("search-key")).toBeNull();
    expect((screen.getByTestId("search-save") as HTMLButtonElement).disabled).toBe(true);
  });

  it("asks for a key for a service, saves it, then forgets the text it was typed into", async () => {
    view();
    await screen.findByTestId("web-search-card");
    choose("Brave Search");
    const save = screen.getByTestId("search-save") as HTMLButtonElement;
    expect(save.disabled).toBe(true);
    const input = screen.getByTestId("search-key") as HTMLInputElement;
    expect(input.type).toBe("password");
    fireEvent.change(input, { target: { value: " BSA-secret " } });
    fireEvent.click(save);
    await waitFor(() => expect(api.set).toHaveBeenCalledWith("brave", "BSA-secret"));
    await waitFor(() => expect((screen.getByTestId("search-key") as HTMLInputElement).value).toBe(""));
    expect(screen.getByTestId("search-status").textContent).toContain("Brave Search");
    expect(document.body.textContent).not.toContain("BSA-secret");
  });

  it("keeps a saved key for the same service but demands a new one for another", async () => {
    api.get.mockResolvedValue({ provider: "brave", has_key: true });
    view();
    await screen.findByTestId("web-search-card");
    expect((screen.getByTestId("search-key") as HTMLInputElement).placeholder).toContain("Key saved");
    expect(screen.getByTestId("search-status").textContent).toContain("Brave Search");
    choose("Tavily");
    expect((screen.getByTestId("search-save") as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(screen.getByTestId("search-key"), { target: { value: "tvly-1" } });
    expect((screen.getByTestId("search-save") as HTMLButtonElement).disabled).toBe(false);
  });

  it("goes back to the free search without a key, and shows a failure's reason", async () => {
    api.get.mockResolvedValue({ provider: "brave", has_key: true });
    api.set.mockResolvedValueOnce({ ok: false, error: "that does not look like an API key" });
    view();
    await screen.findByTestId("web-search-card");
    choose("Free search");
    fireEvent.click(screen.getByTestId("search-save"));
    expect((await screen.findByTestId("search-error")).textContent).toContain("does not look like");
    api.set.mockResolvedValueOnce({ ok: true, provider: "auto", has_key: false });
    fireEvent.click(screen.getByTestId("search-save"));
    await waitFor(() => expect(api.set).toHaveBeenLastCalledWith("auto", undefined));
  });
});
