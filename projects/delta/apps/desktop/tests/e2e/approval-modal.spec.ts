// Approval dialog (design 2:604) in the running app: an outbound action under "Ask each time"
// opens it and stays locked until checked; the preference opens it for any approval, and
// Remember my choice sends a session grant.
import { expect } from "@playwright/test";
import { test, overrideMockCommand } from "./fixtures";

const ASK_EACH_TIME = `(args) => ({ ok: true, default_level: "ask_each_time", is_chosen: true, workspaces: [], endpoint_marks: [], local_only_servers: [], path_overrides: [] })`;

test("outbound action under Ask each time: locked until checked, Escape refuses", async ({ page }) => {
  await overrideMockCommand(page, "get_egress_settings", ASK_EACH_TIME);
  await page.goto("/");
  await page.getByPlaceholder(/Ask Delta/).fill("post the long digest");
  await page.getByRole("button", { name: "Send" }).click();

  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await expect(dialog).toHaveAttribute("aria-modal", "true");
  await expect(dialog.getByRole("button", { name: "Allow once" })).toBeDisabled();
  await page.screenshot({ path: "test-results/approval-modal-outbound.png" });

  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  // The refusal is on the record: the rail's Permissions section marks the request revoked.
  await expect(page.locator('[data-testid="rail-perm-row"][data-state="revoked"]')).toHaveCount(1);
});

test("preference on: a file write opens the dialog; Remember my choice sends a session grant", async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem("delta-approval-dialog", "1"));
  await page.goto("/");
  await page.getByPlaceholder(/Ask Delta/).fill("please write a file");
  await page.getByRole("button", { name: "Send" }).click();

  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await dialog.getByTestId("approval-remember").locator("input").check();
  await dialog.getByRole("button", { name: "Allow once" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByText("Done via write_file [decision=always_tool]")).toBeVisible();
  // After the turn ends the rail still lists the decision, as granted for this session.
  await expect(page.locator('[data-testid="rail-perm-row"][data-state="granted"]')).toHaveCount(1);
});
