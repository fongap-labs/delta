import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@delta/i18n/I18nContext";
import { ApprovalRulesList } from "./ApprovalRulesList";
import { getApprovalRules, revokeApprovalRule, type ApprovalRules } from "../api";

vi.mock("../api", () => ({
  getApprovalRules: vi.fn(),
  revokeApprovalRule: vi.fn(),
}));

const RULES: ApprovalRules = {
  session: [{ tool: "read_file", command: null }],
  workspace: [{ tool: "execute", command: "make check" }],
};
const renderRules = (workspace: string | null = "/work") =>
  render(
    <I18nProvider locale="en-US">
      <ApprovalRulesList sessionId="s-42" workspace={workspace} refreshKey={0} />
    </I18nProvider>,
  );

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(getApprovalRules).mockResolvedValue(RULES);
  vi.mocked(revokeApprovalRule).mockResolvedValue(true);
});
afterEach(cleanup);

describe("approval rule shared controls", () => {
  it("names session and workspace revocation buttons and preserves their exact authorization scope", async () => {
    renderRules();
    const session = await screen.findByRole("button", { name: "Take back: read_file (This session)" });
    const workspace = screen.getByRole("button", { name: "Take back: execute — make check (This workspace)" });
    expect(session.className).toContain("ds-button-danger");
    expect(workspace.className).toContain("ds-button-sm");
    expect(session.getAttribute("type")).toBe("button");
    expect(revokeApprovalRule).not.toHaveBeenCalled();

    fireEvent.click(workspace);
    await waitFor(() => expect(revokeApprovalRule).toHaveBeenCalledWith(
      "workspace", "s-42", "/work", RULES.workspace[0],
    ));
    expect(revokeApprovalRule).toHaveBeenCalledTimes(1);
    expect(getApprovalRules).toHaveBeenCalledWith("s-42", "/work");
  });

  it("disables repeat submissions while one revoke is pending", async () => {
    let complete!: (value: boolean) => void;
    vi.mocked(revokeApprovalRule).mockImplementation(() =>
      new Promise<boolean>((resolve) => { complete = resolve; }),
    );
    renderRules();
    const button = await screen.findByRole("button", { name: "Take back: read_file (This session)" }) as HTMLButtonElement;
    fireEvent.click(button);
    await waitFor(() => expect(button.getAttribute("aria-busy")).toBe("true"));
    expect(button.disabled).toBe(true);
    const other = screen.getByRole("button", { name: "Take back: execute — make check (This workspace)" }) as HTMLButtonElement;
    expect(other.disabled).toBe(true);
    fireEvent.click(other);
    expect(revokeApprovalRule).toHaveBeenCalledTimes(1);
    await act(async () => { complete(true); });
    await waitFor(() => expect(button.disabled).toBe(false));
    expect(getApprovalRules).toHaveBeenCalledTimes(2);
  });

  it("reports rejected revocation and does not claim removal", async () => {
    vi.mocked(revokeApprovalRule).mockRejectedValue(new Error("permission rejected"));
    renderRules(null);
    const button = await screen.findByRole("button", { name: "Take back: read_file (This session)" });
    fireEvent.click(button);
    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Take back: read_file (This session)" })).toBeTruthy();
    expect(getApprovalRules).toHaveBeenCalledTimes(1);
    expect(revokeApprovalRule).toHaveBeenCalledWith("session", "s-42", "", RULES.session[0]);
  });
});
