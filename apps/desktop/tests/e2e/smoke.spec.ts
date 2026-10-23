import { test, expect } from "./fixtures";

test("app loads with the persona nav and composer", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText("Delta").first()).toBeVisible();
  // Search is the fixed top of the sidebar; new task is ⌘N (the collapsed topbar keeps its "+").
  await expect(page.getByTestId("nav-search")).toBeVisible();
  // The sidebar carries no wordmark: the search field leads it (the tool row sits above it).
  await expect(page.locator(".sidebar").getByText("Delta", { exact: true })).toHaveCount(0);
});
