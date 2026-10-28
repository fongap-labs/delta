import { expect } from "@playwright/test";
import { test } from "./fixtures";

// A message typed while a run is going waits above the input. It is sent when the run ends on its
// own, can be changed or dropped meanwhile, and stays put when the user stops the run.

async function startLongRun(page: import("@playwright/test").Page) {
  await page.goto("/");
  await page.getByText("Draft the launch note").first().click();
  const box = page.getByPlaceholder(/Ask Delta/);
  await expect(box).toBeVisible();
  await box.fill("stream the epic");
  await box.press("Enter");
  await expect(page.getByTestId("composer-stop")).toBeVisible();
}

test("a message typed during a run is queued, then sent when the run ends", async ({ page }) => {
  await startLongRun(page);
  const input = page.getByLabel("Message for the current run");
  await input.fill("and then summarize it");
  await input.press("Enter");

  const queued = page.getByTestId("queued-message");
  await expect(queued).toHaveCount(1);
  await expect(queued).toContainText("and then summarize it");

  // The run ends on its own; the queued message goes out as the next turn and leaves the queue.
  await expect(page.getByText("Echo: and then summarize it").first()).toBeVisible({ timeout: 20_000 });
  await expect(page.getByTestId("queued-message")).toHaveCount(0);
});

test("a queued message can be taken back and changed, or dropped", async ({ page }) => {
  await startLongRun(page);
  const input = page.getByLabel("Message for the current run");
  await input.fill("first idea");
  await input.press("Enter");
  await input.fill("second idea");
  await input.press("Enter");
  await expect(page.getByTestId("queued-message")).toHaveCount(2);

  await page.getByTestId("queued-remove").nth(1).click();
  await expect(page.getByTestId("queued-message")).toHaveCount(1);

  await page.getByTestId("queued-edit").first().click();
  await expect(page.getByTestId("queued-message")).toHaveCount(0);
  await expect(input).toHaveValue("first idea");
});

test("stopping a run leaves the queue for the user to decide", async ({ page }) => {
  await startLongRun(page);
  const input = page.getByLabel("Message for the current run");
  await input.fill("do this next");
  await input.press("Enter");
  await expect(page.getByTestId("queued-message")).toHaveCount(1);

  await page.getByTestId("composer-stop").click();
  await expect(page.getByTestId("composer-stop")).toHaveCount(0);
  // Not sent on its own; it is still listed, and sends on request.
  await expect(page.getByTestId("queued-message")).toHaveCount(1);
  await expect(page.getByText("Echo: do this next")).toHaveCount(0);
  await page.getByTestId("queued-interject").click();
  await expect(page.getByText("Echo: do this next").first()).toBeVisible();
  await expect(page.getByTestId("queued-message")).toHaveCount(0);
});
