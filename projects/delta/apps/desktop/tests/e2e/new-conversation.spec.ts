import { test, expect } from "./fixtures";

// The sidebar's "New conversation" entry must open a blank conversation, not just re-show the
// current one: after a turn has been sent, clicking it leaves an empty transcript and a ready composer.

test("New conversation starts a blank conversation after a turn was sent", async ({ page }) => {
  await page.goto("/");
  const box = page.getByPlaceholder(/Ask Delta/);
  await expect(box).toBeVisible();

  await box.fill("hello agent");
  await page.getByRole("button", { name: "Send" }).click();
  await expect(page.getByText(/Echo: hello agent/)).toBeVisible();

  await page.getByTestId("nav-conversations").click();

  await expect(page.getByText(/Echo: hello agent/)).toHaveCount(0);
  await expect(page.getByPlaceholder(/Ask Delta/)).toBeVisible();
  await expect(page.getByPlaceholder(/Ask Delta/)).toHaveValue("");
});
