import { test, expect } from "./fixtures";

// Sidebar organisation: Recent is grouped by date, tasks can be filed into client-side projects,
// and the window-level shortcuts (⌘/Ctrl+K search, ⌘/Ctrl+N new task) work from anywhere.

test("Recent is grouped by date; fixture sessions fall under Earlier", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTitle("Weekly plan 1")).toBeVisible();
  const group = page.getByTestId("recent-group-earlier");
  await expect(group).toContainText("Earlier");
  await expect(group.getByTitle("Weekly plan 1")).toBeVisible();
  // The glance cap still applies across groups.
  await expect(page.getByTitle("Weekly plan 5")).toHaveCount(0);
});

test("a task can be filed into a new project, leaves Recent, survives a reload, and returns on remove", async ({
  page,
}) => {
  await page.goto("/");
  const row = page.getByTitle("Weekly plan 2");
  await expect(row).toBeVisible();

  // Row menu → "New project…" creates the project and files the task in one go.
  await row.hover();
  await row.getByTestId("row-menu").click();
  await row.getByTestId("row-menu-new-project").click();
  await page.getByTestId("new-project-input").fill("Q3 planning");
  await page.getByTestId("new-project-input").press("Enter");

  const band = page.getByTestId("projects-band");
  await expect(band).toContainText("Q3 planning");
  await expect(band.getByTitle("Weekly plan 2")).toBeVisible();
  await expect(page.getByTestId("recent-header").getByTitle("Weekly plan 2")).toHaveCount(0);

  // Persisted on this device.
  await page.reload();
  await expect(page.getByTestId("projects-band").getByTitle("Weekly plan 2")).toBeVisible();

  // Collapse hides its tasks; expand brings them back.
  await page.getByRole("button", { name: "Collapse Q3 planning" }).click();
  await expect(page.getByTestId("projects-band").getByTitle("Weekly plan 2")).toHaveCount(0);
  await page.getByRole("button", { name: "Expand Q3 planning" }).click();
  await expect(page.getByTestId("projects-band").getByTitle("Weekly plan 2")).toBeVisible();

  // Removing the project is two-step and only ungroups: the task is still there, back in Recent.
  const proj = page.getByTestId("projects-band").locator('[data-testid^="project-p_"]').first();
  await proj.getByRole("button", { name: /Collapse Q3 planning/ }).hover();
  const remove = proj.locator('[data-testid^="project-remove-"]');
  await remove.click();
  await expect(remove).toContainText("Tasks are kept");
  await remove.click();
  await expect(page.getByTestId("projects-band")).toHaveCount(0);
  await expect(page.getByTitle("Weekly plan 2")).toHaveCount(1);
});

test("an existing project can be picked from the row menu and cleared again", async ({ page }) => {
  await page.goto("/");
  await page.getByTestId("new-project").click();
  await page.getByTestId("new-project-input").fill("Research");
  await page.getByTestId("new-project-input").press("Enter");
  await expect(page.getByTestId("projects-band")).toContainText("Research");

  const row = page.getByTitle("Weekly plan 3");
  await row.hover();
  await row.getByTestId("row-menu").click();
  await row.locator('[data-testid^="row-menu-project-"]').first().click();
  await expect(page.getByTestId("projects-band").getByTitle("Weekly plan 3")).toBeVisible();

  const filedRow = page.getByTestId("projects-band").getByTitle("Weekly plan 3");
  await filedRow.hover();
  await filedRow.getByTestId("row-menu").click();
  await filedRow.getByTestId("row-menu-no-project").click();
  await expect(page.getByTestId("projects-band").getByTitle("Weekly plan 3")).toHaveCount(0);
  await expect(page.getByTitle("Weekly plan 3")).toHaveCount(1);
});

test("Ctrl+K opens search from anywhere and Esc closes it", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByPlaceholder(/Ask Delta/)).toBeVisible();
  await page.keyboard.press("Control+k");
  const dialog = page.getByRole("dialog");
  await expect(dialog.or(page.getByPlaceholder(/Search/i)).first()).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByPlaceholder(/Search/i)).toHaveCount(0);
});

test("Ctrl+N starts a new task", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTitle("Weekly plan 1")).toBeVisible();
  // The boot session is the pinned "Draft the launch note"; a new task has no active row.
  const active = page.locator(".sidebar [class*=\"bg-ink\"]");
  await expect(active).toHaveCount(1);
  await page.keyboard.press("Control+n");
  await expect(active).toHaveCount(0);
  await expect(page.getByPlaceholder(/Ask Delta/)).toBeVisible();
});

test("Ctrl+Shift+L flips the theme", async ({ page }) => {
  await page.goto("/");
  const before = await page.evaluate(() => document.documentElement.dataset.theme);
  await page.keyboard.press("Control+Shift+L");
  const after = await page.evaluate(() => document.documentElement.dataset.theme);
  expect(after).not.toBe(before);
});
