import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { I18nProvider } from "@delta/i18n/I18nContext";
import { Onboarding } from "./Onboarding";
import { connectConnector, getConnectors, setOnboarded } from "../api";

const providerSetup = vi.hoisted(() => ({ useSetup: vi.fn() }));
vi.mock("../providers/ProviderSetup", () => ({
  useProviderSetup: providerSetup.useSetup,
  ProviderCards: () => null,
  ProviderForm: () => null,
}));
vi.mock("../features/connectors/ConnectorIcon", () => ({
  ConnectorBadge: ({ connector }: { connector: { title: string } }) => <span>{connector.title}</span>,
}));
vi.mock("../api", () => ({
  connectConnector: vi.fn(),
  getConnectors: vi.fn(),
  setOnboarded: vi.fn(),
}));

const onDone = vi.fn();
const makeSetup = (overrides: Record<string, unknown> = {}) => ({
  customProviders: [], orderedCustom: [],
  keylessOk: new Set<string>(["ready"]),
  sel: null, isCreating: false, isDirty: false, secretFilled: false,
  credentialed: false, verify: { state: "idle" },
  openNewCustom: vi.fn(),
  cancelBackTimer: vi.fn(),
  testAndSave: vi.fn().mockResolvedValue(true),
  ...overrides,
});
const view = () => render(
  <I18nProvider locale="en-US">
    <Onboarding onDone={onDone} />
  </I18nProvider>,
);

beforeEach(() => {
  vi.clearAllMocks();
  providerSetup.useSetup.mockReturnValue(makeSetup());
  vi.mocked(getConnectors).mockResolvedValue([]);
  vi.mocked(connectConnector).mockResolvedValue({ ok: true } as never);
  vi.mocked(setOnboarded).mockResolvedValue(undefined as never);
});
afterEach(cleanup);

describe("Onboarding shared action controls", () => {
  it("uses a labelled dialog and does not skip setup without a second confirmation", async () => {
    providerSetup.useSetup.mockReturnValue(makeSetup({ keylessOk: new Set() }));
    view();
    expect(screen.getByRole("dialog", { name: "Welcome to Delta" })).toBeTruthy();

    const next = screen.getByTestId("ob-continue") as HTMLButtonElement;
    expect(next.className).toContain("ds-button-primary");
    expect(next.getAttribute("type")).toBe("button");
    expect(next.disabled).toBe(true);

    fireEvent.click(screen.getByRole("button", { name: "Skip setup" }));
    expect(screen.getByRole("button", { name: "skip anyway" }).className).toContain("ds-button-ghost");
    expect(setOnboarded).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "skip anyway" }));
    await waitFor(() => expect(setOnboarded).toHaveBeenCalledWith(true));
    expect(onDone).toHaveBeenCalledWith(undefined);
  });

  it("checks unsaved model credentials before proceeding to the tools page", async () => {
    const testAndSave = vi.fn().mockResolvedValueOnce(false).mockResolvedValueOnce(true);
    const cancelBackTimer = vi.fn();
    providerSetup.useSetup.mockReturnValue(makeSetup({
      keylessOk: new Set(), sel: "custom", isDirty: true,
      secretFilled: true, credentialed: false, testAndSave, cancelBackTimer,
    }));
    view();
    const next = screen.getByTestId("ob-continue") as HTMLButtonElement;
    expect(next.disabled).toBe(false);
    fireEvent.click(next);
    await waitFor(() => expect(testAndSave).toHaveBeenCalledTimes(1));
    expect(screen.getByTestId("ob-step-model")).toBeTruthy();
    fireEvent.click(next);
    expect(await screen.findByTestId("ob-step-tools")).toBeTruthy();
    expect(testAndSave).toHaveBeenCalledTimes(2);
    expect(cancelBackTimer).toHaveBeenCalledTimes(2);
  });

  it("keeps per-connector explicit consent and retains the automation start choice", async () => {
    vi.mocked(getConnectors).mockResolvedValue([{
      name: "slack", title: "Slack", connected: false,
    }] as never);
    view();
    fireEvent.click(screen.getByTestId("ob-continue"));
    expect(await screen.findByRole("dialog", { name: "Connect your everyday tools" })).toBeTruthy();
    const connect = await screen.findByRole("button", { name: "Connect Slack" });
    expect(connect.className).toContain("ds-button-sm");
    expect(connectConnector).not.toHaveBeenCalled();
    fireEvent.click(connect);
    await waitFor(() => expect(connectConnector).toHaveBeenCalledWith("slack", {}));
    expect(await screen.findByRole("status")).toBeTruthy();

    const next = screen.getByTestId("ob-continue-tools");
    expect(next.className).toContain("ds-button-primary");
    fireEvent.click(next);
    expect(await screen.findByRole("dialog", { name: "You’re set up" })).toBeTruthy();
    const automation = screen.getByTestId("ob-cta-automation");
    const work = screen.getByTestId("ob-start");
    expect(automation.className).toContain("ds-button");
    expect(work.className).toContain("ds-button");
    expect(setOnboarded).not.toHaveBeenCalled();
    fireEvent.click(automation);
    await waitFor(() => expect(setOnboarded).toHaveBeenCalledWith(true));
    expect(onDone).toHaveBeenCalledWith("automations");
  });
});
