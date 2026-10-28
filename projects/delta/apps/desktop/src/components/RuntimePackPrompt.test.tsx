import type { ReactElement } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { I18nProvider } from "@delta/i18n/I18nContext";
import { RuntimePackPrompt } from "./RuntimePackPrompt";

const getRuntimePack = vi.fn();
const installRuntimePack = vi.fn();
vi.mock("../api", () => ({
  getRuntimePack: () => getRuntimePack(),
  installRuntimePack: () => installRuntimePack(),
}));

const published = {
  published: true,
  name: "delta-python",
  version: "0.1.0",
  size: 150 * 1024 * 1024,
  host: "packs.example.com",
  installed: false,
  approved: false,
};

const wrap = (el: ReactElement) => <I18nProvider locale="en-US">{el}</I18nProvider>;

describe("RuntimePackPrompt", () => {
  afterEach(() => {
    cleanup();
    getRuntimePack.mockReset();
    installRuntimePack.mockReset();
  });

  it("says the runtime is not published and offers no install while nothing is published", async () => {
    getRuntimePack.mockResolvedValue({ published: false });
    render(wrap(<RuntimePackPrompt onInstalled={() => {}} />));

    expect(await screen.findByText(/not published yet/)).toBeTruthy();
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("names the exact pack and its host, and installs only after the user clicks", async () => {
    getRuntimePack.mockResolvedValue(published);
    installRuntimePack.mockResolvedValue({ ...published, installed: true, approved: true });
    const onInstalled = vi.fn();
    render(wrap(<RuntimePackPrompt onInstalled={onInstalled} />));

    expect(await screen.findByText(/delta-python 0\.1\.0 \(about 150 MB\) from packs\.example\.com/)).toBeTruthy();
    expect(installRuntimePack).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Download and install" }));

    await waitFor(() => expect(onInstalled).toHaveBeenCalledTimes(1));
    expect(installRuntimePack).toHaveBeenCalledTimes(1);
  });

  it("shows the reason when the install fails and does not report success", async () => {
    getRuntimePack.mockResolvedValue(published);
    installRuntimePack.mockResolvedValue({ published: true, ok: false, error: "the runtime download failed" });
    const onInstalled = vi.fn();
    render(wrap(<RuntimePackPrompt onInstalled={onInstalled} />));

    fireEvent.click(await screen.findByRole("button", { name: "Download and install" }));

    expect(await screen.findByText("the runtime download failed")).toBeTruthy();
    expect(onInstalled).not.toHaveBeenCalled();
  });

  it("renders nothing once the runtime is installed", async () => {
    getRuntimePack.mockResolvedValue({ ...published, installed: true, approved: true });
    const { container } = render(wrap(<RuntimePackPrompt onInstalled={() => {}} />));

    await waitFor(() => expect(getRuntimePack).toHaveBeenCalled());
    expect(container.textContent).toBe("");
  });
});
