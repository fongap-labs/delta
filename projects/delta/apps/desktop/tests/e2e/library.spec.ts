// Settings ▸ Library: put a folder in, refresh it, take it out.
import { expect } from "@playwright/test";
import { patchMockState, readMockState, test } from "./fixtures";

async function openLibrary(page) {
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await expect(page.getByPlaceholder(/Ask Delta/)).toBeVisible();
  await page.getByTestId("sidebar-footer-settings").click();
  await page.getByRole("button", { name: "Library", exact: true }).click();
  await expect(page.getByTestId("library-section")).toBeVisible();
}

test("the library starts empty and says that nothing leaves the computer", async ({ page }) => {
  await openLibrary(page);
  await expect(page.getByTestId("library-empty")).toBeVisible();
  await expect(page.getByTestId("library-section")).toContainText("nothing is sent anywhere");
});

test("adding a folder picks it, indexes it and lists it with its counts", async ({ page }) => {
  await openLibrary(page);
  await page.getByTestId("library-add").click();
  const row = page.getByTestId("library-root");
  await expect(row).toHaveCount(1);
  await expect(row).toContainText("/tmp/picked-folder");
  await expect(row).toContainText("12 files · 48 passages · version 1");
  await expect(row).toContainText("1 skipped");
  await expect(page.getByTestId("library-message")).toContainText("Done: 12 read");
  const state = await readMockState<{ libraryIndexed: string[] }>(page);
  expect(state.libraryIndexed).toEqual(["/tmp/picked-folder"]);
});

test("updating a folder runs the index again and removing it takes it out of the list", async ({ page }) => {
  await patchMockState(page, {
    libraryRoots: [{ root: "/work/docs", version: 3, indexed_at: "2026-10-24T08:00:00Z", files: 5, chunks: 20, skipped: 0 }],
  });
  await openLibrary(page);
  await expect(page.getByTestId("library-root")).toContainText("version 3");
  await page.getByTestId("library-update").click();
  await expect(page.getByTestId("library-root")).toContainText("version 4");
  await page.getByTestId("library-remove").click();
  await expect(page.getByTestId("library-root")).toHaveCount(0);
  await expect(page.getByTestId("library-empty")).toBeVisible();
});
