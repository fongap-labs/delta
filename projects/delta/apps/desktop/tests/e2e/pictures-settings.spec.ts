import { expect, readMockState, test } from "./fixtures";

// Settings ▸ Models ▸ Pictures: which models read pictures, and the vision model for the rest.

async function openPictures(page: import("@playwright/test").Page) {
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await expect(page.getByPlaceholder(/Ask Delta/)).toBeVisible();
  await page.getByTestId("sidebar-footer-settings").click();
  await page.getByRole("button", { name: "Models", exact: true }).click();
  await expect(page.getByTestId("pictures-card")).toBeVisible();
}

test("every model is listed with what is known about it", async ({ page }) => {
  await openPictures(page);
  const rows = page.getByTestId("pictures-models").locator("[data-testid^='pictures-row-']");
  expect(await rows.count()).toBeGreaterThan(0);
  const first = rows.first();
  await expect(first).toContainText("Not known yet");
  await expect(first).toContainText("if it refuses them, Delta sends words instead and remembers");
});

test("marking a model as not reading pictures is saved and shown as set by the user", async ({ page }) => {
  await openPictures(page);
  const row = page.getByTestId("pictures-models").locator("[data-testid^='pictures-row-']").first();
  const choice = row.locator("select");
  const model = await choice.getAttribute("aria-label");
  await choice.selectOption("no");
  await expect(row).toContainText("Does not read pictures · set by you");
  const state = await readMockState<{ visionPatches?: unknown[] }>(page);
  expect(state.visionPatches).toEqual([{ vision_support: { [model!]: false } }]);

  // Back to automatic judging: the mark goes away and the model is unknown again.
  await choice.selectOption("auto");
  await expect(row).toContainText("Not known yet");
  const after = await readMockState<{ visionPatches?: unknown[] }>(page);
  expect(after.visionPatches?.at(-1)).toEqual({ vision_support: { [model!]: null } });
});

test("a vision model can be chosen and removed", async ({ page }) => {
  await openPictures(page);
  const select = page.getByTestId("pictures-vision-model");
  await expect(select).toHaveValue("");
  const firstModel = await page
    .getByTestId("pictures-models")
    .locator("[data-testid^='pictures-row-']")
    .first()
    .locator("select")
    .getAttribute("aria-label");
  await select.selectOption(firstModel!);
  await expect(select).toHaveValue(firstModel!);
  await select.selectOption("");
  const state = await readMockState<{ visionPatches?: { vision_model?: string }[] }>(page);
  expect(state.visionPatches?.map((patch) => patch.vision_model)).toEqual([firstModel, ""]);
});
