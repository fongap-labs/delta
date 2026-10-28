import { expect } from "@playwright/test";
import { test } from "./fixtures";

// While a run waits for the user's decision the status bar says so (icon + words), and returns
// to the running state once the decision is made. The state comes from the pending prompt the
// transcript already holds; nothing is inferred.

test("the status bar shows Awaiting approval while a tool waits and clears when the run ends", async ({ page }) => {
  await page.goto("/");
  const box = page.getByPlaceholder(/Ask Delta/);
  await expect(box).toBeVisible();
  await box.fill("please run a tool");
  await page.getByRole("button", { name: "Send" }).click();

  const bar = page.getByTestId("run-status-bar");
  await expect(page.getByText("The delta wants to run a command.").first()).toBeVisible();
  await expect(bar).toHaveAttribute("data-run-state", "awaiting");
  await expect(bar).toContainText("Awaiting approval");
  await expect(bar).toContainText("Waiting for your decision");

  await page.getByRole("button", { name: "Allow once" }).last().click();
  await expect(page.getByText("The command ran; 1 file found.")).toBeVisible();
  await expect(bar).toHaveCount(0);
});

test("a run that is simply working is shown as Running", async ({ page }) => {
  await page.goto("/");
  await page.getByText("Draft the launch note").first().click();
  const box = page.getByPlaceholder(/Ask Delta/);
  await box.fill("stream the epic");
  await box.press("Enter");
  const bar = page.getByTestId("run-status-bar");
  await expect(bar).toHaveAttribute("data-run-state", "running");
  await expect(bar).toContainText("Running");
});
