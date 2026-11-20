import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { I18nProvider } from "@delta/i18n/I18nContext";
import { InboxConfigure } from "./InboxConfigure";
import {
  getConnectors, getDmRoute, getInboxRouting, getRecentChannels, getSessions,
  getSubscriptions, getUnrouted, setDmRoute, setInboxBinding,
  subscribeChannel, unsubscribeChannel,
} from "../api";

vi.mock("../api", () => ({
  getConnectors: vi.fn(), getDmRoute: vi.fn(), getInboxRouting: vi.fn(),
  getRecentChannels: vi.fn(), getSessions: vi.fn(), getSubscriptions: vi.fn(),
  getUnrouted: vi.fn(), setDmRoute: vi.fn(), setInboxBinding: vi.fn(),
  subscribeChannel: vi.fn(), unsubscribeChannel: vi.fn(),
}));

const session = { session_id: "session-1", title: "Work chat" };

function view() {
  return render(
    <I18nProvider locale="en-US">
      <InboxConfigure />
    </I18nProvider>,
  );
}

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(getConnectors).mockResolvedValue([]);
  vi.mocked(getDmRoute).mockResolvedValue("");
  vi.mocked(getInboxRouting).mockResolvedValue([]);
  vi.mocked(getRecentChannels).mockResolvedValue([]);
  vi.mocked(getSessions).mockResolvedValue([session] as never);
  vi.mocked(getSubscriptions).mockResolvedValue([]);
  vi.mocked(getUnrouted).mockResolvedValue([]);
  vi.mocked(setDmRoute).mockResolvedValue(undefined as never);
  vi.mocked(setInboxBinding).mockResolvedValue({ ok: true });
  vi.mocked(subscribeChannel).mockResolvedValue({ ok: true });
  vi.mocked(unsubscribeChannel).mockResolvedValue({ ok: true });
});
afterEach(cleanup);

describe("InboxConfigure shared routing controls", () => {
  it("keeps mirror routing explicit and provides a labelled DM destination selector", async () => {
    view();
    const set = screen.getByRole("button", { name: "Set" }) as HTMLButtonElement;
    expect(set.className).toContain("ds-button-primary");
    expect(set.getAttribute("type")).toBe("button");
    expect(set.disabled).toBe(true);

    const dm = screen.getByRole("combobox", { name: "Direct messages" }) as HTMLSelectElement;
    expect(dm.className).toContain("ds-field");
    await waitFor(() => expect(dm.options.length).toBe(2));
    fireEvent.change(dm, { target: { value: "session-1" } });
    await waitFor(() => expect(setDmRoute).toHaveBeenCalledWith("session-1"));

    const channel = screen.getAllByPlaceholderText("slack:C0123 or channel link")[0];
    fireEvent.change(channel, { target: { value: "slack:C0123" } });
    expect(set.disabled).toBe(false);
    fireEvent.click(set);
    await waitFor(() => expect(setInboxBinding).toHaveBeenCalledWith("default", "slack", "C0123"));
  });

  it("shows clear-routing errors as alerts without claiming success", async () => {
    vi.mocked(getInboxRouting).mockResolvedValue([
      { name: "default", channel: "slack", target: "C9" },
    ]);
    vi.mocked(setInboxBinding).mockResolvedValue({ ok: false, error: "routing is locked" });
    view();
    const clear = await screen.findByRole("button", { name: "clear" });
    expect(clear.className).toContain("ds-button-danger");
    fireEvent.click(clear);
    await waitFor(() => expect(setInboxBinding).toHaveBeenCalledWith("default", null, ""));
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("routing is locked");
  });

  it("keeps subscription changes explicit and identifies unsubscribe targets", async () => {
    vi.mocked(getSubscriptions).mockResolvedValue([{
      session_id: "session-1", session_title: "Work chat", agent: "default",
      channel: "slack:C99", channel_name: "general",
      routing_target: null, collision: false,
    }]);
    view();
    const unsubscribe = await screen.findByRole("button", { name: "Unsubscribe: general (Work chat)" });
    expect(unsubscribe.className).toContain("ds-button-danger");
    fireEvent.click(unsubscribe);
    await waitFor(() => expect(unsubscribeChannel).toHaveBeenCalledWith("session-1", "slack:C99"));

    const sessionSelect = screen.getByRole("combobox", { name: "Choose a session…" }) as HTMLSelectElement;
    expect(sessionSelect.className).toContain("ds-field");
    const subscribe = screen.getByRole("button", { name: "+ Subscribe" }) as HTMLButtonElement;
    expect(subscribe.disabled).toBe(true);
    fireEvent.change(sessionSelect, { target: { value: "session-1" } });
    fireEvent.change(screen.getAllByPlaceholderText("slack:C0123 or channel link")[1], {
      target: { value: "slack:C123" },
    });
    expect(subscribe.disabled).toBe(false);
    fireEvent.click(subscribe);
    await waitFor(() => expect(subscribeChannel).toHaveBeenCalledWith("session-1", "slack:C123"));
  });

  it("blocks approval routing without an owner via both button and keyboard", async () => {
    vi.mocked(getConnectors).mockResolvedValue([{
      name: "workspace-chat", ui: { detail: "workspace_chat" }, connected: true,
      workspaces: [{ team_id: "T1", approval_owner_ids: [] }],
    }] as never);
    view();
    await waitFor(() => expect(getConnectors).toHaveBeenCalled());
    const channel = screen.getAllByPlaceholderText("slack:C0123 or channel link")[0];
    fireEvent.change(channel, { target: { value: "workspace-chat:T1/C123" } });

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("Choose an approval owner");
    const set = screen.getByRole("button", { name: "Set" }) as HTMLButtonElement;
    expect(set.disabled).toBe(true);
    fireEvent.keyDown(channel, { key: "Enter", code: "Enter" });
    expect(setInboxBinding).not.toHaveBeenCalled();
  });
});
