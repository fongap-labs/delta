// The skills that ship with Delta are read-only: they can be turned off or copied, never edited
// or deleted in place.
import { expect } from "@playwright/test";
import { patchMockState, test } from "./fixtures";

const BUILTIN = {
  name: "wechat-article",
  description: "写公众号文章（WeChat article）",
  instructions: "# 公众号文章\n先问清读者。",
  scope: "builtin",
  source: "builtin",
  enabled: true,
  path: null,
  files: 0,
};

async function openSkills(page) {
  await patchMockState(page, { skills: [BUILTIN] });
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.getByTestId("sidebar-footer-settings").click();
  await page.getByRole("button", { name: "Skills", exact: true }).click();
}

test("a built-in skill is marked, and offers a copy instead of edit and delete", async ({ page }) => {
  await openSkills(page);
  await expect(page.getByText("wechat-article")).toBeVisible();
  await expect(page.getByTestId("skill-builtin-badge")).toHaveText("Built in");
  await expect(page.getByTestId("skill-copy")).toBeVisible();
  await expect(page.getByRole("button", { name: /Delete wechat-article/ })).toHaveCount(0);
});

test("it can be turned off like any skill", async ({ page }) => {
  await openSkills(page);
  const toggle = page.getByRole("switch", { name: "wechat-article enabled" });
  await expect(toggle).toBeChecked();
  await toggle.click();
  await expect(toggle).not.toBeChecked();
});

test("a copy opens the editor with the text filled in under a new name", async ({ page }) => {
  await openSkills(page);
  const editButtons = await page.getByTitle("Edit").count(); // other controls on the page may share the title
  await page.getByTestId("skill-copy").click();
  await expect(page.getByLabel("Name")).toHaveValue("wechat-article-copy");
  await expect(page.getByLabel("Name")).toBeEnabled();
  await expect(page.getByLabel("Instructions")).toHaveValue(/先问清读者/);
  await page.getByRole("button", { name: "Save skill" }).click();
  await expect(page.getByText("wechat-article-copy").first()).toBeVisible();
  // The copy is an ordinary skill and gets the edit button; the built-in one still has none.
  await expect(page.getByTitle("Edit")).toHaveCount(editButtons + 1);
});
