import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactNode } from "react";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { I18nProvider } from "@delta/i18n/I18nContext";
import { EgressChoice } from "./EgressChoice";
import { PrivacySection } from "./PrivacySection";
import {
  getConnectors, getEgressSettings, getMcpServers, pickWorkspaceFolder,
  setEgressDefault, setEndpointMark,
} from "../api";

vi.mock("../api", () => ({
  EGRESS_LEVELS: ["open", "approved_endpoints", "private_only", "ask_each_time"],
  getConnectors: vi.fn(),
  getEgressSettings: vi.fn(),
  getMcpServers: vi.fn(),
  pickWorkspaceFolder: vi.fn(),
  setEgressDefault: vi.fn(),
  setEndpointMark: vi.fn(),
  setLocalOnly: vi.fn(),
  setPathRule: vi.fn(),
  setWorkspaceEgress: vi.fn(),
}));
vi.mock("./WebhookCard", () => ({ WebhookCard: () => null }));

const baseSettings = {
  default_level: "open",
  is_chosen: true,
  workspaces: [{ workspace: "/work", level: "private_only" }],
  endpoint_marks: [{ host: "gateway.example.com", mark: "approved" }],
  local_only_servers: [],
  path_overrides: [],
};
const wrap = (element: ReactNode) =>
  render(<I18nProvider locale="en-US">{element}</I18nProvider>);

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(getEgressSettings).mockResolvedValue(baseSettings as never);
  vi.mocked(getConnectors).mockResolvedValue([]);
  vi.mocked(getMcpServers).mockResolvedValue([]);
  vi.mocked(pickWorkspaceFolder).mockResolvedValue(null);
});
afterEach(cleanup);

describe("shared privacy egress controls", () => {
  it("requires an explicit first-run choice before saving the egress policy", async () => {
    const onChosen = vi.fn();
    vi.mocked(setEgressDefault).mockResolvedValue({ ...baseSettings, ok: true, default_level: "private_only" } as never);
    wrap(<EgressChoice onChosen={onChosen} />);
    const dialog = screen.getByRole("dialog", { name: "Where may your work be sent?" });
    const levels = screen.getByRole("radiogroup", { name: "Where may your work be sent?" });
    const apply = screen.getByRole("button", { name: "Use this level" }) as HTMLButtonElement;
    expect(dialog).toBeTruthy();
    expect(levels).toBeTruthy();
    expect(apply.className).toContain("ds-button-primary");
    expect(apply.disabled).toBe(true);
    expect(setEgressDefault).not.toHaveBeenCalled();

    fireEvent.click(screen.getByTestId("egress-level-private_only"));
    expect(apply.disabled).toBe(false);
    fireEvent.click(apply);
    await waitFor(() => expect(setEgressDefault).toHaveBeenCalledWith("private_only", false));
    expect(onChosen).toHaveBeenCalledOnce();
  });

  it("leaves an egress loosening unapplied when cancelled and applies only after confirmation", async () => {
    const restrictedSettings = { ...baseSettings, default_level: "private_only" };
    vi.mocked(getEgressSettings).mockResolvedValue(restrictedSettings as never);
    vi.mocked(setEgressDefault).mockImplementation(async (level, confirmed) => (
      confirmed
        ? { ...restrictedSettings, default_level: level, ok: true }
        : { ok: false, needs_confirmation: true }
    ) as never);
    wrap(<PrivacySection />);
    expect(await screen.findByRole("radiogroup", { name: "Default for workspaces" })).toBeTruthy();
    const open = screen.getByTestId("egress-level-open");
    fireEvent.click(open);
    expect(await screen.findByRole("dialog", { name: "Allow content to go further?" })).toBeTruthy();
    const cancel = screen.getByRole("button", { name: "Cancel" });
    expect(cancel.className).toContain("ds-button");
    fireEvent.click(cancel);
    expect(screen.queryByRole("dialog", { name: "Allow content to go further?" })).toBeNull();
    expect(setEgressDefault).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("egress-level-private_only").querySelector("input")?.checked).toBe(true);

    fireEvent.click(open);
    const confirm = await screen.findByRole("button", { name: "Confirm" });
    expect(confirm.className).toContain("ds-button-primary");
    fireEvent.click(confirm);
    await waitFor(() => expect(setEgressDefault).toHaveBeenLastCalledWith("open", true));
    await waitFor(() => expect(screen.getByTestId("egress-level-open").querySelector("input")?.checked).toBe(true));
  });

  it("uses labelled workspace/path fields and retains explicit endpoint removal", async () => {
    vi.mocked(setEndpointMark).mockResolvedValue({ ...baseSettings, ok: true, endpoint_marks: [] } as never);
    wrap(<PrivacySection />);
    const workspace = await screen.findByRole("combobox", { name: "/work: Workspaces with their own level" });
    expect(workspace.className).toContain("ds-field");
    const pathSelector = screen.getByRole("combobox", { name: "Rules for each tool" });
    expect(pathSelector.className).toContain("ds-field");

    const host = screen.getByRole("textbox", { name: "gateway.example.com" }) as HTMLInputElement;
    expect(host.className).toContain("ds-field");
    const mark = screen.getByRole("button", { name: "Mark" }) as HTMLButtonElement;
    expect(mark.disabled).toBe(true);
    fireEvent.change(host, { target: { value: "new.example.com" } });
    expect(mark.disabled).toBe(false);
    const remove = screen.getByRole("button", { name: "Remove" });
    expect(remove.className).toContain("ds-button-danger");
    fireEvent.click(remove);
    await waitFor(() => expect(setEndpointMark).toHaveBeenCalledWith("gateway.example.com", null, false));
    await waitFor(() => expect(screen.queryByTestId("egress-endpoint")).toBeNull());
  });
});
