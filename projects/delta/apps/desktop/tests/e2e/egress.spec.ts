import { expect } from "@playwright/test";
import { overrideMockCommand, test } from "./fixtures";

// The egress level (ADR-0057): the first-run choice, and the Privacy tab that changes it later.
// Nothing is sent to a model until a default level is chosen, so the chooser must appear when no
// level is chosen, and must go away once one is.

const UNCHOSEN = `(args) => ({ ok: true, default_level: null, is_chosen: false, workspaces: [], endpoint_marks: [] })`;

async function openApp(page: import("@playwright/test").Page) {
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await expect(page.getByPlaceholder(/Ask Delta/)).toBeVisible();
}

test("first run asks for a level, preselects none, and closes once one is chosen", async ({ page }) => {
  await overrideMockCommand(page, "get_egress_settings", UNCHOSEN);
  await openApp(page);
  const chooser = page.getByTestId("egress-choice");
  await expect(chooser).toBeVisible();
  for (const level of ["open", "approved_endpoints", "private_only", "ask_each_time"]) {
    await expect(page.getByTestId(`egress-level-${level}`).locator("input")).not.toBeChecked();
  }
  await page.getByTestId("egress-level-private_only").click();
  await page.getByRole("button", { name: "Use this level" }).click();
  await expect(chooser).toHaveCount(0);
});

test("the Privacy tab shows the default and each workspace level", async ({ page }) => {
  await openApp(page);
  await page.getByTestId("sidebar-footer-settings").click();
  await page.getByRole("button", { name: "Privacy", exact: true }).click();
  await expect(page.getByTestId("privacy-section")).toBeVisible();
  await expect(page.getByTestId("egress-level-open").locator("input")).toBeChecked();
});

test("a change the runtime wants confirmed waits for confirmation, and cancel keeps the old level", async ({ page }) => {
  // The mock asks for confirmation on every change until it is confirmed, as the runtime does for a loosening.
  await overrideMockCommand(
    page,
    "set_egress_default",
    `(args) => (args.isConfirmed ? { ok: true, default_level: args.level, is_chosen: true, workspaces: [], endpoint_marks: [] } : { ok: false, needs_confirmation: true })`,
  );
  await openApp(page);
  await page.getByTestId("sidebar-footer-settings").click();
  await page.getByRole("button", { name: "Privacy", exact: true }).click();
  await page.getByTestId("egress-level-private_only").click();
  await expect(page.getByTestId("egress-confirm")).toBeVisible();
  await page.getByRole("button", { name: "Cancel" }).click();
  await expect(page.getByTestId("egress-confirm")).toHaveCount(0);
  await expect(page.getByTestId("egress-level-open").locator("input")).toBeChecked();
});
