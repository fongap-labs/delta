// The circuit breaker (A3): an automation the runtime paused after repeated denials says why,
// and the denial limit is a setting the user can change.
import { expect } from "@playwright/test";
import { patchMockState, readMockState, test } from "./fixtures";

const PAUSED = {
  id: "task-paused",
  title: "Weekly report",
  instructions: "Write the weekly report",
  schedule: "Every Monday at ~9:00 AM",
  workspace: "",
  enabled: false,
  next_run: null,
  last_run: null,
  last_status: null,
  run_count: 2,
  notify_on_completion: false,
  always_allowed: [],
  paused_reason: "approval_denied",
  paused_rule: "write_report",
};

async function openAutomations(page) {
  await page.goto("/");
  await page.getByTestId("nav-automations").click();
  await expect(page.getByText("Recurring tasks Delta runs on a schedule.")).toBeVisible();
}

test("a paused automation says it was paused for repeated denials", async ({ page }) => {
  await patchMockState(page, { automations: [PAUSED] });
  await openAutomations(page);
  const card = page.locator(".sched-card", { hasText: "Weekly report" });
  await expect(card).toContainText("Paused — write_report was denied too many times");
  await card.click();
  await expect(page.getByTestId("task-paused-denied")).toContainText("write_report");
  await expect(page.getByTestId("task-paused-denied")).toContainText("Turn it back on");
});

test("an automation paused by hand is not blamed on denials", async ({ page }) => {
  await patchMockState(page, {
    automations: [{ ...PAUSED, paused_reason: undefined, paused_rule: undefined }],
  });
  await openAutomations(page);
  const card = page.locator(".sched-card", { hasText: "Weekly report" });
  await expect(card).not.toContainText("denied");
  await card.click();
  await expect(page.getByTestId("task-paused-denied")).toHaveCount(0);
});

test("the denial limit defaults to 3 and the user can change it", async ({ page }) => {
  await openAutomations(page);
  const select = page.getByTestId("denial-limit").locator("select");
  await expect(select).toHaveValue("3");
  await select.selectOption("5");
  await expect(select).toHaveValue("5");
  const state = await readMockState<{ settings: { denial_threshold?: number } }>(page);
  expect(state.settings.denial_threshold).toBe(5);
});
