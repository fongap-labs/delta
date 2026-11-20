import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { I18nProvider } from "@delta/i18n/I18nContext";
import { getConnectors, getRecentChannels, connectConnector } from "../api";
import { AutomationQuickstart } from "./AutomationQuickstart";

vi.mock("../api", () => ({
  getConnectors: vi.fn(),
  getRecentChannels: vi.fn(),
  connectConnector: vi.fn(),
}));
vi.mock("../features/connectors/ConnectorIcon", () => ({
  ConnectorBadge: () => <span aria-hidden="true" />,
}));
vi.mock("./SubscriptionsChip", () => ({
  ChannelPicker: ({ value, onChange }: { value: string; onChange: (value: string) => void }) => (
    <input aria-label="Channel" value={value} onChange={(event) => onChange(event.target.value)} />
  ),
}));
vi.mock("./SelectMenu", () => ({
  SelectMenu: ({ value, options, onChange, ariaLabel }: {
    value: string;
    options: { value: string; label: string }[];
    onChange: (value: string) => void;
    ariaLabel: string;
  }) => (
    <select aria-label={ariaLabel} value={value} onChange={(event) => onChange(event.target.value)}>
      {options.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
    </select>
  ),
}));

function view(onCreate = vi.fn(), busy = false) {
  return render(
    <I18nProvider locale="en-US">
      <AutomationQuickstart busy={busy} onCreate={onCreate} />
    </I18nProvider>,
  );
}

beforeEach(() => {
  vi.resetAllMocks();
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
    configurable: true, value: vi.fn(),
  });
  vi.mocked(getConnectors).mockResolvedValue([]);
  vi.mocked(getRecentChannels).mockResolvedValue([]);
  vi.mocked(connectConnector).mockResolvedValue({ ok: true } as never);
});
afterEach(cleanup);

describe("automation quickstart shared controls", () => {
  it("chooses a card without creating anything and only submits after explicit confirmation", async () => {
    const onCreate = vi.fn();
    view(onCreate);
    const template = screen.getByTestId("qs-template-news");
    expect(template.className).toContain("ds-button");
    expect(template.getAttribute("type")).toBe("button");
    expect(template.getAttribute("aria-pressed")).toBe("false");
    expect(onCreate).not.toHaveBeenCalled();
    fireEvent.click(template);
    expect(template.getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByTestId("qs-configure")).toBeTruthy();
    const create = screen.getByTestId("ob-create") as HTMLButtonElement;
    expect(create.className).toContain("ds-button-primary");
    expect(create.disabled).toBe(false);
    expect(onCreate).not.toHaveBeenCalled();
    fireEvent.click(create);
    expect(onCreate).toHaveBeenCalledTimes(1);
    expect(onCreate).toHaveBeenCalledWith({
      title: "Morning news briefing",
      instructions: expect.any(String),
      cron: "0 8 * * *",
      permissions: [],
    });
  });

  it("retains scheduling input semantics while using the shared field and button", () => {
    const onCreate = vi.fn();
    view(onCreate);
    fireEvent.click(screen.getByTestId("qs-template-cleanup"));
    const time = screen.getByLabelText("Time") as HTMLInputElement;
    expect(time.className).toContain("ds-field");
    fireEvent.change(time, { target: { value: "13:15" } });
    fireEvent.click(screen.getByTestId("ob-create"));
    expect(onCreate).toHaveBeenCalledWith({
      title: "Folder cleanup",
      instructions: expect.any(String),
      cron: "15 13 * * 5",
      permissions: [],
    });
  });

  it("requires connected services and an explicit channel before enabling a write recipe", async () => {
    const onCreate = vi.fn();
    view(onCreate);
    fireEvent.click(screen.getByTestId("qs-template-github"));
    const create = screen.getByTestId("ob-create") as HTMLButtonElement;
    expect(create.disabled).toBe(true);
    const connect = await screen.findByTestId("ob-connect-github");
    expect(connect.className).toContain("ds-button");
    expect(onCreate).not.toHaveBeenCalled();
    fireEvent.click(connect);
    await waitFor(() => expect(connectConnector).toHaveBeenCalledWith("github", {}));
    expect(onCreate).not.toHaveBeenCalled();
    expect(create.disabled).toBe(true);
  });

  it("keeps write permission tied to the consent checkbox and chosen channel", async () => {
    vi.mocked(getConnectors).mockResolvedValue([
      { name: "slack", title: "Slack", connected: true, logo: "slack", brand_color: "#999" },
      { name: "github", title: "GitHub", connected: true, logo: "github", brand_color: "#999" },
    ] as never);
    const onCreate = vi.fn();
    view(onCreate);
    fireEvent.click(screen.getByTestId("qs-template-github"));
    const recipe = await screen.findByTestId("ob-recipe");
    expect(recipe).toBeTruthy();
    const create = screen.getByTestId("ob-create") as HTMLButtonElement;
    expect(create.disabled).toBe(true);
    fireEvent.change(screen.getByRole("textbox", { name: "Repository" }), {
      target: { value: "fongap-labs/delta" },
    });
    fireEvent.change(screen.getByRole("textbox", { name: "Channel" }), {
      target: { value: "slack:C01" },
    });
    expect(create.disabled).toBe(false);
    const consent = screen.getByTestId("ob-hasConsent") as HTMLInputElement;
    fireEvent.click(consent);
    expect(consent.checked).toBe(false);
    fireEvent.click(create);
    expect(onCreate).toHaveBeenLastCalledWith(expect.objectContaining({ permissions: [] }));
    fireEvent.click(consent);
    fireEvent.click(create);
    expect(onCreate).toHaveBeenLastCalledWith(expect.objectContaining({
      permissions: [{ tool: "send_message", target: "slack:C01", access: "write" }],
    }));
  });

  it("disables the action when parent is already creating an automation", () => {
    const onCreate = vi.fn();
    view(onCreate, true);
    fireEvent.click(screen.getByTestId("qs-template-news"));
    const create = screen.getByTestId("ob-create") as HTMLButtonElement;
    expect(create.disabled).toBe(true);
    expect(create.getAttribute("aria-busy")).toBe("true");
    fireEvent.click(create);
    expect(onCreate).not.toHaveBeenCalled();
  });
});
