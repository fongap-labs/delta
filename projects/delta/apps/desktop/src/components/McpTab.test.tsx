import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { I18nProvider } from "@delta/i18n/I18nContext";
import { ConnectSetup, McpTab } from "./ManageTabs";
import { addMcpServer, connectConnector, connectMcpBacked, getEgressSettings, getMcpServers, getMcpTools, reloadMcp } from "../api";

vi.mock("../api", () => ({
  addMcpServer: vi.fn(),
  connectConnector: vi.fn(),
  connectMcpBacked: vi.fn(),
  getMcpServers: vi.fn(),
  getEgressSettings: vi.fn(),
  getMcpTools: vi.fn(),
  reloadMcp: vi.fn(),
  connectMcp: vi.fn(),
  deleteMcpServer: vi.fn(),
  patchMcpServer: vi.fn(),
  signoutMcp: vi.fn(),
}));

const SERVER = {
  name: "demo",
  transport: "http",
  auth: "none",
  status: "connected",
  enabled: true,
  config: { type: "http", url: "https://example.com/mcp" },
  tool_count: 1,
  requires_approval: true,
  last_error: null,
};

function renderTab() {
  return render(
    <I18nProvider locale="en-US">
      <McpTab />
    </I18nProvider>,
  );
}

beforeEach(() => {
  // McpRow renders EndpointMarkNote, which reads egress settings on mount.
  vi.mocked(getEgressSettings).mockResolvedValue({ endpoint_marks: [] } as never);
  vi.mocked(getMcpServers).mockResolvedValue([SERVER] as never);
  vi.mocked(getMcpTools).mockResolvedValue({ ok: true, tools: [{ name: "search_content", description: "Search" }] } as never);
  vi.mocked(addMcpServer).mockResolvedValue({ ok: true } as never);
  vi.mocked(reloadMcp).mockResolvedValue(undefined as never);
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("MCP management shared controls", () => {
  it("links its editor disclosure and validates JSON before saving", async () => {
    renderTab();
    const edit = await screen.findByTestId("mcp-edit-demo");
    expect(edit.className).toContain("ds-button-ghost");
    expect(edit.getAttribute("type")).toBe("button");
    expect(edit.getAttribute("aria-expanded")).toBe("false");

    fireEvent.click(edit);
    expect(edit.getAttribute("aria-expanded")).toBe("true");
    expect(document.getElementById(edit.getAttribute("aria-controls")!)).toBeTruthy();

    const input = screen.getByTestId("mcp-config-demo") as HTMLTextAreaElement;
    expect(input.className).toContain("ds-field");
    fireEvent.change(input, { target: { value: "{" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(addMcpServer).not.toHaveBeenCalled();

    fireEvent.change(input, { target: { value: '{"type":"http","url":"https://example.com/mcp"}' } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(addMcpServer).toHaveBeenCalledWith("demo", { type: "http", url: "https://example.com/mcp" }));
    await waitFor(() => expect(edit.getAttribute("aria-expanded")).toBe("false"));
    expect(screen.getByRole("button", { name: "Remove" }).className).toContain("ds-button-danger");
  });

  it("exposes a labelled tools region with a stable button name", async () => {
    renderTab();
    const toggle = await screen.findByTestId("mcp-tools-demo");
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    expect(toggle.getAttribute("aria-busy")).toBe("false");

    fireEvent.click(toggle);
    await screen.findByText("search_content");
    expect(getMcpTools).toHaveBeenCalledWith("demo");
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    expect(document.getElementById(toggle.getAttribute("aria-controls")!)).toBeTruthy();

    fireEvent.click(toggle);
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByText("search_content")).toBeNull();
  });

  it("validates custom MCP JSON before adding a server", async () => {
    vi.mocked(getMcpServers).mockResolvedValue([]);
    renderTab();
    const open = await screen.findByRole("button", { name: /Add a custom MCP server/i });
    fireEvent.click(open);

    const input = screen.getByTestId("mcp-add-json") as HTMLTextAreaElement;
    expect(input.className).toContain("ds-field");
    fireEvent.change(input, { target: { value: "invalid" } });
    fireEvent.click(screen.getByTestId("mcp-add-submit"));
    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(addMcpServer).not.toHaveBeenCalled();

    fireEvent.change(input, { target: { value: '{"filesystem":{"command":"npx","args":["--version"]}}' } });
    fireEvent.click(screen.getByTestId("mcp-add-submit"));
    await waitFor(() => expect(addMcpServer).toHaveBeenCalledWith("filesystem", { command: "npx", args: ["--version"] }));
    await waitFor(() => expect(screen.queryByTestId("mcp-add-json")).toBeNull());
  });
});


describe("ConnectSetup — shared connection controls", () => {
  const connector = {
    name: "example", title: "Example", mcp: false, instructions: [],
    fields: [{ key: "token", label: "API token", secret: true, required: true }],
  };

  it("only starts a manual connector authorization after an explicit Connect click", async () => {
    const onConnected = vi.fn();
    vi.mocked(connectConnector).mockResolvedValue({ ok: true } as never);
    render(
      <I18nProvider locale="en-US">
        <ConnectSetup c={connector as never} onConnected={onConnected} />
      </I18nProvider>,
    );
    const token = screen.getByLabelText("API token") as HTMLInputElement;
    expect(token.type).toBe("password");
    expect(token.className).toContain("ds-field");
    const connect = screen.getByRole("button", { name: "Connect" });
    expect(connect.className).toContain("ds-button-primary");
    expect(connect.getAttribute("type")).toBe("button");
    expect(connectConnector).not.toHaveBeenCalled();
    fireEvent.change(token, { target: { value: "test-value" } });
    fireEvent.click(connect);
    await waitFor(() => expect(connectConnector).toHaveBeenCalledWith("example", { token: "test-value" }));
    await waitFor(() => expect(onConnected).toHaveBeenCalledTimes(1));
  });

  it("keeps unsuccessful connection attempts visible without claiming success", async () => {
    const onConnected = vi.fn();
    vi.mocked(connectConnector).mockResolvedValue({ ok: false, error: "Connection rejected" } as never);
    render(
      <I18nProvider locale="en-US">
        <ConnectSetup c={connector as never} onConnected={onConnected} />
      </I18nProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Connect" }));
    const error = await screen.findByRole("alert");
    expect(error.textContent).toContain("Connection rejected");
    expect(onConnected).not.toHaveBeenCalled();
  });

  it("keeps one-click MCP sign-in as an explicit action", async () => {
    vi.mocked(connectMcpBacked).mockResolvedValue({ ok: true } as never);
    const managed = { ...connector, mcp: true, fields: [] };
    render(
      <I18nProvider locale="en-US">
        <ConnectSetup c={managed as never} onConnected={vi.fn()} />
      </I18nProvider>,
    );
    const oneClick = screen.getByRole("button", { name: /Connect Example with one click/ });
    expect(oneClick.className).toContain("ds-button-primary");
    expect(connectMcpBacked).not.toHaveBeenCalled();
    fireEvent.click(oneClick);
    await waitFor(() => expect(connectMcpBacked).toHaveBeenCalledWith("example"));
    expect((oneClick as HTMLButtonElement).disabled).toBe(true);
  });
});
