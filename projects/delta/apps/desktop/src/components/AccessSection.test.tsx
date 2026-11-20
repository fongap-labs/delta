import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@delta/i18n/I18nContext";
import { AccessSection } from "./AccessSection";
import {
  getConnectors, getRecentChannels, getSessionConnections, getSubscriptions,
  setSessionConnection, subscribeChannel, unsubscribeChannel,
  type Connector, type SessionConnections, type Subscription,
} from "../api";
import { useRoots } from "../useRoots";

vi.mock("../api", () => ({
  getConnectors: vi.fn(),
  getRecentChannels: vi.fn(),
  getSessionConnections: vi.fn(),
  getSubscriptions: vi.fn(),
  setSessionConnection: vi.fn(),
  subscribeChannel: vi.fn(),
  unsubscribeChannel: vi.fn(),
}));
vi.mock("../useRoots", () => ({ useRoots: vi.fn() }));
vi.mock("../features/connectors/ConnectorIcon", () => ({
  ConnectorBadge: () => <span aria-hidden="true" />,
}));
vi.mock("./ManageTabs", () => ({
  ConnectSetup: () => <div data-testid="connect-setup">Connector setup</div>,
}));
vi.mock("./SubscriptionsChip", () => ({
  ChannelPicker: ({ value, onChange }: { value: string; onChange: (s: string) => void }) =>
    <input aria-label="Channel address" value={value} onChange={(e) => onChange(e.target.value)} />,
}));
vi.mock("./RootRow", () => ({ RootRow: () => <div data-testid="root-row" /> }));
vi.mock("./AddFolderForm", () => ({
  AddFolderForm: ({ onAdd }: { onAdd: (path: string, writable: boolean) => Promise<boolean> }) =>
    <button data-testid="folder-add-readonly" onClick={() => void onAdd("/work/docs", false)}>Grant read-only</button>,
}));
vi.mock("./Toggle", () => ({
  Toggle: ({ checked, onChange, title }: { checked: boolean; onChange: (v: boolean) => void; title: string }) =>
    <button role="switch" aria-label={title} aria-checked={checked} onClick={() => onChange(!checked)}>Session toggle</button>,
}));

const CONNECTORS: Connector[] = [
  {
    name: "slack", title: "Slack", icon: "", logo: "slack",
    brand_color: "#999999", blurb: "Chat", auth: "oauth", two_way: true,
    channels: true, available: true, connected: true, account: null,
    enabled: true, fields: [], instructions: [], allowed_users: [],
    tools: [], managed: false, managed_profile: false,
  },
  {
    name: "github", title: "GitHub", icon: "", logo: "github",
    brand_color: "#999999", blurb: "Repositories", aliases: ["source code"],
    auth: "oauth", two_way: false, channels: false, available: true,
    connected: false, account: null, enabled: true, fields: [], instructions: [],
    allowed_users: [], tools: [], managed: false, managed_profile: false,
  },
];
const CONNECTIONS: SessionConnections = {
  connected: [{ connector: "slack", enabled: true, detail: "Team account" }],
  recommended: [{ connector: "github", reason: "Useful for reviews", tier: "core", connected: false }],
  attention: 1,
};
const CHANNEL: Subscription = {
  session_id: "s42", session_title: "Test", agent: "delta",
  channel: "slack:C01", channel_name: "general", routing_target: null, collision: false,
};
const onOpenIntegrations = vi.fn();
function view() {
  return render(
    <I18nProvider locale="en-US">
      <AccessSection sessionId="s42" onOpenIntegrations={onOpenIntegrations} />
    </I18nProvider>,
  );
}
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(getSessionConnections).mockResolvedValue(CONNECTIONS);
  vi.mocked(getConnectors).mockResolvedValue(CONNECTORS);
  vi.mocked(getSubscriptions).mockResolvedValue([CHANNEL]);
  vi.mocked(getRecentChannels).mockResolvedValue([]);
  vi.mocked(setSessionConnection).mockResolvedValue({ ok: true });
  vi.mocked(subscribeChannel).mockResolvedValue({ ok: true, channel: "slack:C02" });
  vi.mocked(unsubscribeChannel).mockResolvedValue({ ok: true, removed: true });
  vi.mocked(useRoots).mockReturnValue({
    roots: [], isBusy: false, error: "", addRoot: vi.fn().mockResolvedValue(true),
    toggleAccess: vi.fn(), removeRoot: vi.fn(), reload: vi.fn(),
  });
});
afterEach(cleanup);

describe("session access shared controls", () => {
  it("shows explicit connector discovery without granting global or session access on display", async () => {
    view();
    const add = await screen.findByTestId("access-add-source");
    expect(add.className).toContain("ds-button-ghost");
    expect(add.getAttribute("type")).toBe("button");
    expect(setSessionConnection).not.toHaveBeenCalled();
    expect(subscribeChannel).not.toHaveBeenCalled();
    fireEvent.click(add);
    const search = screen.getByRole("textbox", { name: "Search connectors…" }) as HTMLInputElement;
    expect(search.className).toContain("ds-field");
    const github = await screen.findByTestId("access-add-github");
    expect(github.className).toContain("ds-button-ghost");
    fireEvent.change(search, { target: { value: "source code" } });
    expect(screen.getByTestId("access-add-github")).toBeTruthy();
    expect(screen.queryByTestId("access-add-slack")).toBeNull();
    expect(setSessionConnection).not.toHaveBeenCalled();
    fireEvent.keyDown(search, { key: "Escape" });
    expect(screen.getByTestId("access-add-source")).toBeTruthy();
  });

  it("keeps session connector muting separate from global connector setup", async () => {
    view();
    const toggle = await screen.findByRole("switch", { name: /Enabled for this session/ });
    fireEvent.click(toggle);
    await waitFor(() => expect(setSessionConnection).toHaveBeenCalledWith("s42", "slack", false));
    const connect = await screen.findByRole("button", { name: "Connect: GitHub" });
    expect(connect.className).toContain("ds-button-primary");
    expect(connect.getAttribute("type")).toBe("button");
    expect(onOpenIntegrations).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Manage all connectors (global) →" }));
    expect(onOpenIntegrations).toHaveBeenCalledTimes(1);
    expect(subscribeChannel).not.toHaveBeenCalled();
  });

  it("preserves per-session channel targeting and never invokes global connector mutation", async () => {
    view();
    const channels = await screen.findByRole("button", { name: /Channels · 1/ });
    fireEvent.click(channels);
    const back = screen.getByRole("button", { name: "Back to connectors" });
    expect(back.className).toContain("ds-button-ghost");
    const remove = screen.getByRole("button", { name: "Stop listening: #general" });
    expect(remove.className).toContain("ds-button-ghost");
    fireEvent.click(remove);
    await waitFor(() => expect(unsubscribeChannel).toHaveBeenCalledWith("s42", "slack:C01"));
    expect(setSessionConnection).not.toHaveBeenCalled();
    fireEvent.change(screen.getByRole("textbox", { name: "Channel address" }), { target: { value: "C02" } });
    const add = screen.getByRole("button", { name: "Add" }) as HTMLButtonElement;
    expect(add.className).toContain("ds-button-primary");
    fireEvent.click(add);
    await waitFor(() => expect(subscribeChannel).toHaveBeenCalledWith("s42", "slack:C02"));
    expect(setSessionConnection).not.toHaveBeenCalled();
  });

  it("only begins a read-only folder grant after choosing the explicit access action", async () => {
    view();
    const folder = await screen.findByRole("button", { name: "+ Give access to a folder…" });
    expect(folder.className).toContain("ds-button-ghost");
    expect(screen.queryByTestId("folder-add-readonly")).toBeNull();
    fireEvent.click(folder);
    fireEvent.click(screen.getByTestId("folder-add-readonly"));
    const mockResults = vi.mocked(useRoots).mock.results;
    const impl = mockResults[mockResults.length - 1]?.value;
    expect(impl.addRoot).toHaveBeenCalledWith("/work/docs", false);
    expect(setSessionConnection).not.toHaveBeenCalled();
  });
});
