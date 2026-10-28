import { expect, overrideMockCommand, readMockState, test } from "./fixtures";

// About names the maker and links to its website.
test("About shows who makes Delta and links to the Fongap Labs website", async ({ page }) => {
  await overrideMockCommand(
    page,
    "plugin:opener|open_url",
    `(args) => { $state.opened = [...($state.opened || []), args.url]; return null; }`,
  );
  await page.goto("/");
  await page.getByTestId("sidebar-footer-settings").click();
  await page.getByRole("button", { name: "About", exact: true }).click();

  await expect(page.getByTestId("about-maker")).toContainText("Developed by Fongap Labs");
  const link = page.getByTestId("about-website");
  await expect(link).toHaveText("labs.fongap.com");
  await expect(link).toHaveAttribute("href", "https://labs.fongap.com");

  // The link goes to the system browser through the opener, not into the app window.
  await link.click();
  await expect
    .poll(async () => (await readMockState<{ opened?: string[] }>(page)).opened)
    .toEqual(["https://labs.fongap.com"]);
  await expect(page.getByTestId("about-maker")).toBeVisible();
});
