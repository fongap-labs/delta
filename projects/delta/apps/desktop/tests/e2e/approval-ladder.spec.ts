// The approval ladder (A3): what the user allowed without being asked is listed in the
// Permissions section, with its scope, and each item can be taken back.
import { expect } from "@playwright/test";
import { patchMockState, readMockState, test } from "./fixtures";

const SESSION_RULE = { tool: "write_file", command: null };
const WORKSPACE_RULES = [
  { tool: "run_shell", command: "git status", created_at: "2026-10-25T08:00:00Z" },
  { tool: "create_document", command: null, created_at: "2026-10-25T08:05:00Z" },
];

async function openSession(page) {
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.getByText("Draft the launch note").first().click();
}

test("nothing is listed when nothing was allowed without asking", async ({ page }) => {
  await openSession(page);
  await expect(page.getByTestId("approval-rules")).toHaveCount(0);
});

test("session and workspace allowances are listed with their scope", async ({ page }) => {
  await patchMockState(page, { rulesSession: [SESSION_RULE], rulesWorkspace: WORKSPACE_RULES });
  await openSession(page);
  const rows = page.getByTestId("approval-rule");
  await expect(rows).toHaveCount(3);
  await expect(rows.nth(0)).toContainText("write_file");
  await expect(rows.nth(0).getByTestId("approval-rule-scope")).toHaveText("This session");
  await expect(rows.nth(1)).toContainText("run_shell");
  await expect(rows.nth(1)).toContainText("git status");
  await expect(rows.nth(1).getByTestId("approval-rule-scope")).toHaveText("This workspace");
});

test("taking back a session allowance removes it and tells the runtime", async ({ page }) => {
  await patchMockState(page, { rulesSession: [SESSION_RULE], rulesWorkspace: WORKSPACE_RULES });
  await openSession(page);
  await page.getByTestId("approval-rule").first().getByTestId("approval-rule-revoke").click();
  await expect(page.getByTestId("approval-rule")).toHaveCount(2);
  const state = await readMockState<{ revoked: { scope: string; tool: string; command: string | null }[] }>(page);
  expect(state.revoked).toEqual([{ scope: "session", tool: "write_file", command: null }]);
});

test("taking back one command rule leaves the others", async ({ page }) => {
  await patchMockState(page, { rulesSession: [], rulesWorkspace: WORKSPACE_RULES });
  await openSession(page);
  await page.getByTestId("approval-rule").filter({ hasText: "git status" }).getByTestId("approval-rule-revoke").click();
  await expect(page.getByTestId("approval-rule")).toHaveCount(1);
  await expect(page.getByTestId("approval-rule")).toContainText("create_document");
  const state = await readMockState<{ revoked: { scope: string; tool: string; command: string | null }[] }>(page);
  expect(state.revoked).toEqual([{ scope: "workspace", tool: "run_shell", command: "git status" }]);
});
