import { expect } from "@playwright/test";
import { overrideMockCommand, readMockState, test } from "./fixtures";

// Regenerate on the last reply reuses the revert that powers Edit: the thread is cut back to
// before the last user message and the same text is sent again for a fresh answer.

const REVERT = `(args) => {
  const messages = $state.sessionMessages[args.sessionId] || [];
  const text = messages[args.index] ? messages[args.index].content : "";
  $state.sessionMessages[args.sessionId] = messages.slice(0, args.index);
  $state.revertCalls = ($state.revertCalls || []).concat([{ sessionId: args.sessionId, index: args.index }]);
  return { ok: true, text };
}`;

test("Regenerate re-runs the last user message and replaces the reply", async ({ page }) => {
  await overrideMockCommand(page, "session_revert", REVERT);
  await page.goto("/");
  const box = page.getByPlaceholder(/Ask Delta/);
  await expect(box).toBeVisible();
  await box.fill("regen me");
  await box.press("Enter");
  await expect(page.getByText("Echo: regen me").first()).toBeVisible();

  // Only the final answer of an idle thread offers it.
  const regenerate = page.getByTestId("bubble-regenerate");
  await expect(regenerate).toHaveCount(1);
  await page.getByText("Echo: regen me").first().hover();
  await regenerate.click();

  // The runtime cut the thread back to before the user message, then the text ran again.
  await expect(page.getByText("Echo: regen me")).toHaveCount(1);
  await expect(page.getByText("regen me", { exact: true })).toHaveCount(1);
  const state = await readMockState<{ revertCalls?: { index: number }[] }>(page);
  expect(state.revertCalls?.length).toBe(1);
});

test("Regenerate is not offered while a run is in progress", async ({ page }) => {
  await page.goto("/");
  const box = page.getByPlaceholder(/Ask Delta/);
  await expect(box).toBeVisible();
  await box.fill("stream the epic");
  await box.press("Enter");
  await expect(page.getByText("The epic scrolls ever onward").first()).toBeVisible({ timeout: 10_000 });
  await expect(page.getByTestId("bubble-regenerate")).toHaveCount(0);
});
