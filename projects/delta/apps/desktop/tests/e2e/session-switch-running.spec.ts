// Owner-hit (issue #7): "switching conversations interrupts the task". It does not — the
// runtime keeps the turn running and only the foreground socket is torn down. What broke
// was the VIEW: on return, `turn_start` had long since been emitted to nobody, so nothing
// set `isRunning` and the composer went back to Send while the turn was still streaming.
// The UI now asks the runtime (`runtime_state`) what is actually alive.
import { expect } from "@playwright/test";
import { test } from "./fixtures";

test("switching away and back keeps a running turn running", async ({ page }) => {
  await page.goto("/");
  await page.getByText("Draft the launch note").first().click();
  const box = page.getByPlaceholder(/Ask Delta/);
  await box.fill("stream the epic");
  await box.press("Enter");

  // The stream is live, so Stop is offered.
  await expect(page.getByText("The epic scrolls ever onward").first()).toBeVisible({
    timeout: 10_000,
  });
  await expect(page.getByRole("button", { name: /Stop/ })).toBeVisible();

  // Leave for another conversation while it streams.
  await page.getByTitle("Weekly plan 1").click();
  await expect(page.getByRole("button", { name: /Stop/ })).toHaveCount(0);

  // Come back mid-turn: the run is still alive, so the composer must offer Stop again
  // and keep following the stream — not sit there looking idle.
  await page.getByTitle("Draft the launch note").click();
  await expect(page.getByRole("button", { name: /Stop/ })).toBeVisible();
  await expect(page.getByText("The epic scrolls ever onward").first()).toBeVisible();
});

test("coming back to a session whose turn finished shows it idle", async ({ page }) => {
  await page.goto("/");
  await page.getByText("Draft the launch note").first().click();
  const box = page.getByPlaceholder(/Ask Delta/);
  await box.fill("stream the epic");
  await box.press("Enter");
  await expect(page.getByText("The epic concludes.").first()).toBeVisible({ timeout: 20_000 });
  await expect(page.getByRole("button", { name: /Stop/ })).toHaveCount(0);

  // Away and back AFTER the turn ended: no phantom Stop button, transcript intact.
  await page.getByTitle("Weekly plan 1").click();
  await page.getByTitle("Draft the launch note").click();
  await expect(page.getByRole("button", { name: /Stop/ })).toHaveCount(0);
  await expect(page.getByText("The epic concludes.").first()).toBeVisible();
});
