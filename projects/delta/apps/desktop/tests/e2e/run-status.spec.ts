import { expect } from "@playwright/test";
import { test } from "./fixtures";

// The run's state is written out in one place: the status line at the foot of the transcript.
// While a run waits for the user's decision that line says so (icon + words) and goes away when
// the run ends. There is no status bar above the composer; the composer's button says Stop.

test("the status line shows the wait for a decision and clears when the run ends", async ({ page }) => {
  await page.goto("/");
  const box = page.getByPlaceholder(/Ask Delta/);
  await expect(box).toBeVisible();
  await box.fill("please run a tool");
  await page.getByRole("button", { name: "Send" }).click();

  const status = page.getByTestId("live-run-status");
  await expect(page.getByText("The delta wants to run a command.").first()).toBeVisible();
  await expect(status).toHaveAttribute("data-run-state", "awaiting");
  await expect(status).toContainText("Waiting for your decision");
  await expect(page.getByTestId("run-status-bar")).toHaveCount(0);

  await page.getByRole("button", { name: "Allow once" }).last().click();
  await expect(page.getByText("The command ran; 1 file found.")).toBeVisible();
  await expect(status).toHaveCount(0);
});

test("a run that is working offers Stop and has no status bar above the composer", async ({ page }) => {
  await page.goto("/");
  await page.getByText("Draft the launch note").first().click();
  const box = page.getByPlaceholder(/Ask Delta/);
  await box.fill("stream the epic");
  await box.press("Enter");
  await expect(page.getByRole("button", { name: "Stop" })).toBeVisible();
  await expect(page.getByTestId("run-status-bar")).toHaveCount(0);
});
