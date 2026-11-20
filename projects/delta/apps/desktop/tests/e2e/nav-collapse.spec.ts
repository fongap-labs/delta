// R6 left-nav polish: collapse (⌘B / left edge tab → the same tab docks it back) and the
// identity-collapsed Recent list, with no legacy persona grouping controls.
import { expect } from "@playwright/test";
import { test } from "./fixtures";

test("collapse hides the sidebar and reclaims the width; the edge tab docks it back", async ({
  page,
}) => {
  await page.goto("/");
  const app = page.locator(".app");
  await expect(page.locator(".sidebar")).toBeVisible();

  // Collapse via the left edge tab, the only sidebar toggle besides ⌘B.
  await expect(page.getByRole("button", { name: "Collapse sidebar" })).toHaveCount(0);
  await page.getByRole("button", { name: "Hide sidebar" }).click();
  await expect(app).toHaveClass(/nav-collapsed/);
  // Nothing else floats in to reveal it: the same tab, now saying "Show sidebar", docks it back.
  await expect(page.locator(".nav-reveal-btn")).toHaveCount(0);
  const reveal = page.getByRole("button", { name: "Show sidebar" });
  await expect(reveal).toHaveCount(1);
  await reveal.click();
  await expect(app).not.toHaveClass(/nav-collapsed/);
});

test("⌘B toggles the sidebar collapse", async ({ page }) => {
  await page.goto("/");
  // Wait for boot to finish: the ⌘B keydown handler mounts only after the splash clears,
  // so a press fired mid-boot is silently dropped (the very first `.app` paint carries
  // `boot-splash`). Same guard as session-shell.spec.ts.
  await expect(page.locator(".app")).not.toHaveClass(/boot-splash/);
  const app = page.locator(".app");
  await page.keyboard.press("Meta+b");
  await expect(app).toHaveClass(/nav-collapsed/);
  await page.keyboard.press("Meta+b");
  await expect(app).not.toHaveClass(/nav-collapsed/);
});

test("Recent is a fixed flat list without legacy persona grouping controls", async ({
  page,
}) => {
  await page.goto("/");
  const header = page.getByTestId("recent-header");
  await expect(header).toContainText("Recent");

  await expect(header.getByRole("button", { name: "Group and filter conversations" })).toHaveCount(0);
  await expect(page.getByTestId("group-filter-menu")).toHaveCount(0);
  await expect(page.getByTitle("Weekly plan 1")).toBeVisible();
});
