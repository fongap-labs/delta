import { expect } from "@playwright/test";
import { overrideMockCommand, patchMockState, test } from "./fixtures";

// Loading, "could not load" and empty are three different states. A failed read used to clear
// the list and show the empty message; it must now say it failed and offer a retry.

const FAIL = `(args) => { throw new Error("boom"); }`;

async function openActivity(page: import("@playwright/test").Page) {
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await expect(page.getByPlaceholder(/Ask Delta/)).toBeVisible();
  await page.getByRole("button", { name: /Activity/ }).first().click();
}

test("Activity: a failed read says so and Retry loads the records", async ({ page }) => {
  // The command fails while `auditFail` is set; flipping it live lets Retry succeed.
  await overrideMockCommand(
    page,
    "audit_list",
    `(args) => { if ($state.auditFail) throw new Error("boom"); return { events: [{ id: 1, timestamp: "2026-10-10 10:00:00", session_id: "s1", agent: "delta", workspace: "w", connector: "gmail", tool: "send_message", stage: "executed", status: "ok", approval: "approved", args: {}, result_preview: "", reason: "", resource: "" }] }; }`,
  );
  await patchMockState(page, { auditFail: true });
  await openActivity(page);
  await expect(page.getByTestId("load-error")).toContainText("Couldn't load activity");
  await expect(page.getByText("No audit events yet.")).toHaveCount(0);

  await page.evaluate(() => { (window as any).__DELTA_MOCK__.state.auditFail = false; });
  await page.getByRole("button", { name: "Retry" }).click();
  await expect(page.getByTestId("load-error")).toHaveCount(0);
  await expect(page.getByText("send_message")).toBeVisible();
});

test("Activity: an empty history is the empty message, not an error", async ({ page }) => {
  await openActivity(page);
  await expect(page.getByText("No audit events yet.")).toBeVisible();
  await expect(page.getByTestId("load-error")).toHaveCount(0);
});

test("Activity: a slow read shows a skeleton, then the result", async ({ page }) => {
  await overrideMockCommand(page, "audit_list", `(args) => new Promise((resolve) => setTimeout(() => resolve({ events: [] }), 1800))`);
  await openActivity(page);
  await expect(page.locator(".skeleton-row").first()).toBeVisible({ timeout: 1500 });
  await expect(page.getByText("No audit events yet.")).toBeVisible({ timeout: 5000 });
  await expect(page.locator(".skeleton-row")).toHaveCount(0);
});

test("Inbox: a failed first read says so instead of showing an empty queue", async ({ page }) => {
  await overrideMockCommand(page, "inbox_list", FAIL);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await expect(page.getByPlaceholder(/Ask Delta/)).toBeVisible();
  await page.getByRole("button", { name: /Inbox/ }).first().click();
  await expect(page.getByTestId("load-error")).toContainText("Couldn't load the inbox");
});

test("Memory: a failed read says so instead of 'Nothing yet'", async ({ page }) => {
  await overrideMockCommand(page, "memory_list", FAIL);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await expect(page.getByPlaceholder(/Ask Delta/)).toBeVisible();
  await page.getByRole("button", { name: /Settings/ }).first().click();
  await page.getByRole("button", { name: /^Memory/ }).first().click();
  await expect(page.getByTestId("load-error")).toContainText("Couldn't load memory");
});
