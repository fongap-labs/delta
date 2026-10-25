// The completion webhook card in Settings ▸ Privacy: save a URL, switch it on, and send a test.
import { expect } from "@playwright/test";
import { readMockState, test } from "./fixtures";

async function openCard(page) {
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await expect(page.getByPlaceholder(/Ask Delta/)).toBeVisible();
  await page.getByTestId("sidebar-footer-settings").click();
  await page.getByRole("button", { name: "Privacy", exact: true }).click();
  await expect(page.getByTestId("webhook-card")).toBeVisible();
}

test("the webhook is off and empty at first, and cannot be saved or tested until something changes", async ({ page }) => {
  await openCard(page);
  await expect(page.getByTestId("webhook-url")).toHaveValue("");
  await expect(page.getByTestId("webhook-enabled")).not.toBeChecked();
  await expect(page.getByTestId("webhook-details")).not.toBeChecked();
  await expect(page.getByTestId("webhook-save")).toBeDisabled();
  await expect(page.getByTestId("webhook-test")).toBeDisabled();
});

test("a saved URL can be tested, and the test says what the egress level did", async ({ page }) => {
  await openCard(page);
  await page.getByTestId("webhook-url").fill("https://hooks.example/delta");
  await page.getByTestId("webhook-enabled").check();
  await expect(page.getByTestId("webhook-test")).toBeDisabled(); // unsaved edits are not tested
  await page.getByTestId("webhook-save").click();
  await expect(page.getByTestId("webhook-message")).toHaveText("Saved.");
  const state = await readMockState<{ webhook: { url: string; is_enabled: boolean; include_details: boolean } }>(page);
  expect(state.webhook).toEqual({ url: "https://hooks.example/delta", is_enabled: true, include_details: false });
  await page.getByTestId("webhook-test").click();
  await expect(page.getByTestId("webhook-message")).toHaveText("Sent to hooks.example (HTTP 200).");
});

test("details are off until asked for, and the saved choice is kept", async ({ page }) => {
  await openCard(page);
  await page.getByTestId("webhook-url").fill("https://hooks.example/delta");
  await page.getByTestId("webhook-details").check();
  await page.getByTestId("webhook-save").click();
  await expect(page.getByTestId("webhook-message")).toHaveText("Saved.");
  const state = await readMockState<{ webhook: { include_details: boolean; is_enabled: boolean } }>(page);
  expect(state.webhook.include_details).toBe(true);
  expect(state.webhook.is_enabled).toBe(false);
});

test("an address the runtime refuses is reported and nothing is saved", async ({ page }) => {
  await openCard(page);
  await page.getByTestId("webhook-url").fill("ftp://hooks.example/delta");
  await page.getByTestId("webhook-save").click();
  await expect(page.getByTestId("webhook-message")).toContainText("must start with http");
  const state = await readMockState<{ webhook?: unknown }>(page);
  expect(state.webhook).toBeUndefined();
});
