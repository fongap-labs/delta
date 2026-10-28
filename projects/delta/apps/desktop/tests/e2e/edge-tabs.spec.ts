import { test, expect, overrideMockCommand } from "./fixtures";

// Each side panel has one handle on its edge: the same spot closes it and brings it back, and the
// chevron points the way the panel will move.

test("the left edge tab hides and restores the sidebar", async ({ page }) => {
  await page.goto("/");
  const tab = page.getByTestId("edge-tab-nav");
  await expect(tab).toHaveAttribute("aria-label", "Hide sidebar");
  await expect(tab).toHaveAttribute("aria-expanded", "true");
  await expect(page.locator(".app")).not.toHaveClass(/nav-collapsed/);

  await tab.click();
  await expect(page.locator(".app")).toHaveClass(/nav-collapsed/);
  await expect(tab).toHaveAttribute("aria-label", "Show sidebar");
  await expect(tab).toHaveAttribute("aria-expanded", "false");

  // Pointing at the collapsed tab peeks the sidebar; a click docks it again.
  await tab.click();
  await expect(page.locator(".app")).not.toHaveClass(/nav-collapsed/);
});

test("the right edge tab hides and restores the side panel", async ({ page }) => {
  await page.goto("/");
  const tab = page.getByTestId("edge-tab-rail");
  await expect(tab).toHaveAttribute("aria-label", "Hide side panel");
  await expect(page.locator(".right-rail")).toBeVisible();

  await tab.click();
  await expect(page.locator(".right-rail")).toHaveCount(0);
  await expect(tab).toHaveAttribute("aria-label", "Show side panel");

  await tab.click();
  await expect(page.locator(".right-rail")).toBeVisible();
});

test("side panel sections open by themselves only when they have something to show", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator(".right-rail")).toBeVisible();
  // Progress and Artifacts are empty; Permissions has the connectors the fixtures switch on.
  await expect(page.locator(".rail-section-toggle")).toHaveCount(3);
  await expect(page.getByTestId("access-section")).toBeVisible();
  await expect(page.locator(".rail-section-body")).toHaveCount(1);
  await expect(page.getByTestId("access-section").getByText("Connectors", { exact: true })).toBeVisible();
});

test("the side panel has no second toggle in the topbar once it is hidden", async ({ page }) => {
  await page.goto("/");
  await page.getByTestId("edge-tab-rail").click();
  await expect(page.locator(".right-rail")).toHaveCount(0);
  await expect(page.locator(".main-topbar-actions button")).toHaveCount(0);
});

test("the first-run chooser covers the edge tabs", async ({ page }) => {
  await overrideMockCommand(
    page,
    "get_egress_settings",
    `(args) => ({ ok: true, default_level: null, is_chosen: false, workspaces: [], endpoint_marks: [] })`,
  );
  await page.goto("/");
  const chooser = page.getByTestId("egress-choice");
  await expect(chooser).toBeVisible();
  const zIndex = (selector: string) =>
    page.locator(selector).first().evaluate((el) => Number(getComputedStyle(el).zIndex));
  expect(await zIndex(".edge-tab-left")).toBeLessThan(await zIndex(".gate-overlay"));
  expect(await zIndex(".edge-tab-right")).toBeLessThan(await zIndex(".gate-overlay"));
});

test("the sidebar's top row keeps every tool on one line", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator(".sidebar .brand")).toBeVisible();
  const centres = await page.locator(".sidebar .brand > *").evaluateAll((els) =>
    els.map((el) => {
      const box = el.getBoundingClientRect();
      return Math.round(box.top + box.height / 2);
    }),
  );
  expect(centres.length).toBeGreaterThanOrEqual(5);
  expect(new Set(centres).size).toBe(1);
});
