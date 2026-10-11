import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { I18nProvider } from "@delta/i18n/I18nContext";
import type { SearchEngine } from "../api";

const api = vi.hoisted(() => ({ get: vi.fn(), save: vi.fn(), test: vi.fn() }));
vi.mock("../api", async (original) => ({
  ...(await original<typeof import("../api")>()),
  getWebSearchEngines: api.get,
  saveWebSearchEngines: api.save,
  testWebSearchEngine: api.test,
}));

import { WebSearchCard } from "./SettingsView";

const engine = (id: string, name: string, over: Partial<SearchEngine> = {}): SearchEngine => ({
  id, name, builtin: true, enabled: true, needs_key: false, accepts_key: true,
  has_key: false, has_cx: false, template: "", auth_header: "", ...over,
});
const defaults = (): SearchEngine[] => [
  engine("google", "Google"),
  engine("bing", "Bing", { accepts_key: false }),
  engine("duckduckgo", "DuckDuckGo", { accepts_key: false }),
  engine("brave", "Brave Search", { enabled: false, needs_key: true }),
  engine("tavily", "Tavily", { enabled: false, needs_key: true }),
];

const view = () =>
  render(
    <I18nProvider locale="en-US">
      <WebSearchCard />
    </I18nProvider>,
  );
const saveButton = () => screen.getByTestId("search-save") as HTMLButtonElement;

beforeEach(() => {
  api.get.mockReset().mockResolvedValue(defaults());
  api.save.mockReset().mockImplementation(async () => ({ ok: true, engines: defaults() }));
  api.test.mockReset().mockResolvedValue({ ok: true, count: 5, first: "Rust" });
});
afterEach(cleanup);

describe("web search engines", () => {
  it("lists the three free engines on and the keyed two off, with nothing to save yet", async () => {
    view();
    await screen.findByTestId("web-search-card");
    const on = (id: string) => (screen.getByTestId(`search-enabled-${id}`) as HTMLInputElement).checked;
    expect(["google", "bing", "duckduckgo", "brave", "tavily"].map(on)).toEqual([true, true, true, false, false]);
    expect(screen.getByTestId("search-engine-brave").textContent).toContain("needs a key");
    expect(saveButton().disabled).toBe(true);
  });

  it("marks the first engine that is on as the default and lets any usable engine take that place", async () => {
    view();
    await screen.findByTestId("web-search-card");
    expect(screen.getByTestId("search-default-google")).toBeTruthy();
    expect(screen.queryByTestId("search-default-bing")).toBeNull();
    // Brave has no key, so it cannot be the default.
    expect(screen.queryByTestId("search-makedefault-brave")).toBeNull();
    fireEvent.click(screen.getByTestId("search-makedefault-duckduckgo"));
    expect(screen.getByTestId("search-default-duckduckgo")).toBeTruthy();
    expect(screen.queryByTestId("search-default-google")).toBeNull();
    fireEvent.click(saveButton());
    await waitFor(() => expect(api.save).toHaveBeenCalled());
    const sent = api.save.mock.calls[0][0] as { id: string }[];
    expect(sent.map((e) => e.id).slice(0, 3)).toEqual(["duckduckgo", "google", "bing"]);
  });

  it("makes a switched-off engine the default by turning it on", async () => {
    api.get.mockResolvedValue(defaults().map((e) => (e.id === "bing" ? { ...e, enabled: false } : e)));
    view();
    await screen.findByTestId("web-search-card");
    fireEvent.click(screen.getByTestId("search-makedefault-bing"));
    expect((screen.getByTestId("search-enabled-bing") as HTMLInputElement).checked).toBe(true);
    expect(screen.getByTestId("search-default-bing")).toBeTruthy();
  });

  it("reorders and switches engines off, then saves the whole list in that order", async () => {
    view();
    await screen.findByTestId("web-search-card");
    fireEvent.click(screen.getByTestId("search-down-google"));
    fireEvent.click(screen.getByTestId("search-enabled-bing"));
    expect(saveButton().disabled).toBe(false);
    fireEvent.click(saveButton());
    await waitFor(() => expect(api.save).toHaveBeenCalled());
    const sent = api.save.mock.calls[0][0] as { id: string; enabled: boolean }[];
    expect(sent.map((e) => e.id)).toEqual(["bing", "google", "duckduckgo", "brave", "tavily"]);
    expect(sent[0].enabled).toBe(false);
    await waitFor(() => expect(saveButton().disabled).toBe(true));
  });

  it("sends a typed key once, keeps it out of the page afterwards, and can remove a saved one", async () => {
    api.get.mockResolvedValue(defaults().map((e) => (e.id === "brave" ? { ...e, has_key: true } : e)));
    view();
    await screen.findByTestId("web-search-card");
    fireEvent.click(screen.getByTestId("search-edit-tavily"));
    const input = screen.getByTestId("search-key-tavily") as HTMLInputElement;
    expect(input.type).toBe("password");
    fireEvent.change(input, { target: { value: "tvly-secret" } });
    fireEvent.click(screen.getByTestId("search-edit-brave"));
    fireEvent.click(screen.getByTestId("search-clearkey-brave"));
    fireEvent.click(saveButton());
    await waitFor(() => expect(api.save).toHaveBeenCalled());
    const sent = api.save.mock.calls[0][0] as { id: string; api_key?: string; clear_key?: boolean }[];
    expect(sent.find((e) => e.id === "tavily")?.api_key).toBe("tvly-secret");
    expect(sent.find((e) => e.id === "brave")?.clear_key).toBe(true);
    // Untouched engines send no key at all, which keeps the saved one.
    expect(sent.find((e) => e.id === "google")).not.toHaveProperty("api_key");
    await waitFor(() => expect(document.body.textContent).not.toContain("tvly-secret"));
  });

  it("adds an engine of the user's own, only with a name and an address containing %s once", async () => {
    view();
    await screen.findByTestId("web-search-card");
    fireEvent.click(screen.getByTestId("search-add"));
    const confirm = screen.getByTestId("search-add-confirm") as HTMLButtonElement;
    fireEvent.change(screen.getByTestId("search-add-name"), { target: { value: "My SearXNG" } });
    fireEvent.change(screen.getByTestId("search-add-template"), { target: { value: "https://searx.example/search?q=" } });
    expect(confirm.disabled).toBe(true);
    fireEvent.change(screen.getByTestId("search-add-template"), { target: { value: "https://searx.example/search?q=%s&format=json" } });
    expect(confirm.disabled).toBe(false);
    fireEvent.click(confirm);
    fireEvent.click(saveButton());
    await waitFor(() => expect(api.save).toHaveBeenCalled());
    const sent = api.save.mock.calls[0][0] as Record<string, unknown>[];
    const mine = sent[sent.length - 1];
    expect(mine).toMatchObject({ name: "My SearXNG", template: "https://searx.example/search?q=%s&format=json", enabled: true });
    expect(mine).not.toHaveProperty("id");
  });

  it("shows why a save was refused and keeps the edits", async () => {
    api.save.mockResolvedValueOnce({ ok: false, error: "Brave Search needs an API key before it can be turned on" });
    view();
    await screen.findByTestId("web-search-card");
    fireEvent.click(screen.getByTestId("search-enabled-brave"));
    fireEvent.click(saveButton());
    expect((await screen.findByTestId("search-error")).textContent).toContain("needs an API key");
    expect((screen.getByTestId("search-enabled-brave") as HTMLInputElement).checked).toBe(true);
    expect(saveButton().disabled).toBe(false);
  });

  it("tests a saved engine and reports what it found; unsaved edits must be saved first", async () => {
    view();
    await screen.findByTestId("web-search-card");
    fireEvent.click(screen.getByTestId("search-edit-bing"));
    fireEvent.click(screen.getByTestId("search-test-bing"));
    expect((await screen.findByTestId("search-test-result-bing")).textContent).toContain("5 results");
    expect(api.test).toHaveBeenCalledWith("bing");
    fireEvent.click(screen.getByTestId("search-enabled-google"));
    expect((screen.getByTestId("search-test-bing") as HTMLButtonElement).disabled).toBe(true);
  });
});
