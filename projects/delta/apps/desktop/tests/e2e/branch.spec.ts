import { expect } from "@playwright/test";
import { readMockState, test } from "./fixtures";

// "Branch in new conversation" on an answer copies the thread up to that answer into a new
// session and opens it. The original keeps its whole thread.

test("branching an answer opens a new conversation that ends at that answer", async ({ page }) => {
  await page.goto("/");
  const box = page.getByPlaceholder(/Ask Delta/);
  await expect(box).toBeVisible();
  await box.fill("first question");
  await box.press("Enter");
  await expect(page.getByText("Echo: first question").first()).toBeVisible();
  await box.fill("second question");
  await box.press("Enter");
  await expect(page.getByText("Echo: second question").first()).toBeVisible();

  // Branch from the FIRST answer, not the latest one.
  const branch = page.getByTestId("bubble-branch");
  await expect(branch).toHaveCount(2);
  await page.getByText("Echo: first question").first().hover();
  await branch.first().click();

  // The branch is open: it has the first exchange only, under a title that says what it is.
  await expect(page.getByText("Echo: first question").first()).toBeVisible();
  await expect(page.getByText("Echo: second question")).toHaveCount(0);
  await expect(page.locator(".main-title-text")).toContainText("(branch)");
  const state = await readMockState<{ branchCalls?: { index: number }[] }>(page);
  expect(state.branchCalls?.length).toBe(1);
});

test("a branch is not offered while a run is in progress", async ({ page }) => {
  await page.goto("/");
  const box = page.getByPlaceholder(/Ask Delta/);
  await expect(box).toBeVisible();
  await box.fill("stream the epic");
  await box.press("Enter");
  await expect(page.getByText("The epic scrolls ever onward").first()).toBeVisible({ timeout: 10_000 });
  await expect(page.getByTestId("bubble-branch")).toHaveCount(0);
});
