import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@delta/i18n/I18nContext";
import { WorkspaceTrustPrompt } from "./WorkspaceTrustPrompt";
import { setWorkspaceTrusted, type WorkspaceCommandTrust } from "../api";

vi.mock("../api", () => ({ setWorkspaceTrusted: vi.fn() }));

const REQUEST: WorkspaceCommandTrust = {
  workspace: "/work/delta",
  requested_commands: ["pnpm test", "cargo check"],
  trusted: false,
  required: true,
  exists: true,
};

const onClose = vi.fn();
function view() {
  return render(
    <I18nProvider locale="en-US">
      <WorkspaceTrustPrompt request={REQUEST} onClose={onClose} />
    </I18nProvider>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(setWorkspaceTrusted).mockResolvedValue({ ok: true } as never);
});
afterEach(cleanup);

describe("workspace trust confirmation", () => {
  it("shows requested commands and never trusts a workspace until explicitly confirmed", () => {
    view();
    const dialog = screen.getByRole("dialog", { name: "Trust this workspace’s commands?" });
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(dialog.getAttribute("aria-describedby")).toBe("workspace-trust-description");
    expect(within(dialog).getByText("pnpm test")).toBeTruthy();
    expect(within(dialog).getByText("cargo check")).toBeTruthy();
    expect(within(dialog).getByText("/work/delta")).toBeTruthy();
    const cancel = screen.getByRole("button", { name: "Keep asking" });
    expect(cancel.className).toContain("ds-button-sm");
    expect(setWorkspaceTrusted).not.toHaveBeenCalled();
    fireEvent.click(cancel);
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(setWorkspaceTrusted).not.toHaveBeenCalled();
  });

  it("only dismisses after a successful explicit trust API result", async () => {
    view();
    const approve = screen.getByRole("button", { name: "Trust workspace" });
    expect(approve.className).toContain("ds-button-primary");
    expect(approve.getAttribute("type")).toBe("button");
    fireEvent.click(approve);
    await waitFor(() => expect(setWorkspaceTrusted).toHaveBeenCalledWith("/work/delta", true));
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
    expect(setWorkspaceTrusted).toHaveBeenCalledTimes(1);
  });

  it("prevents duplicate trust requests while a save is pending", async () => {
    let finish!: (result: Awaited<ReturnType<typeof setWorkspaceTrusted>>) => void;
    vi.mocked(setWorkspaceTrusted).mockImplementation(() =>
      new Promise((resolve) => { finish = resolve; }),
    );
    view();
    const approve = screen.getByRole("button", { name: "Trust workspace" }) as HTMLButtonElement;
    const cancel = screen.getByRole("button", { name: "Keep asking" }) as HTMLButtonElement;
    fireEvent.click(approve);
    expect(approve.disabled).toBe(true);
    expect(approve.getAttribute("aria-busy")).toBe("true");
    expect(cancel.disabled).toBe(true);
    fireEvent.click(approve);
    expect(setWorkspaceTrusted).toHaveBeenCalledTimes(1);
    expect(onClose).not.toHaveBeenCalled();
    await act(async () => { finish({ ok: true } as never); });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("announces a failed save and permits retry without closing prematurely", async () => {
    vi.mocked(setWorkspaceTrusted)
      .mockRejectedValueOnce(new Error("connection lost"))
      .mockResolvedValueOnce({ ok: true } as never);
    view();
    const approve = screen.getByRole("button", { name: "Trust workspace" });
    fireEvent.click(approve);
    const error = await screen.findByRole("alert");
    expect(error.textContent).toContain("Could not save workspace trust");
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.click(approve);
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
    expect(setWorkspaceTrusted).toHaveBeenCalledTimes(2);
  });
});
