import { expect } from "@playwright/test";
import { overrideMockCommand, readMockState, test } from "./fixtures";

// The card under a finished request can undo every file the request wrote, as one. It asks first,
// undoes the run the request started, and says what it did.

const MESSAGES = `(args) => ({ messages: [
  { role: "user", content: "write the report", run_id: "run-7", ts: 1760000000 },
  { role: "assistant", content: "", tool_calls: [{ id: "c1", function: { name: "write_file", arguments: JSON.stringify({ path: "report.md" }) } }] },
  { role: "tool", tool_call_id: "c1", content: JSON.stringify({ ok: true, artifacts: [{ path: "report.md" }] }) },
  { role: "assistant", content: "The report is written.", ts: 1760000005 }
] })`;

test("a finished request's file changes are undone after a confirmation", async ({ page }) => {
  await overrideMockCommand(page, "session_messages", MESSAGES);
  await page.goto("/");
  await page.getByText("Draft the launch note").first().click();
  await expect(page.getByTestId("delivery-card")).toBeVisible();

  await page.getByTestId("undo-run-files").click();
  await expect(page.getByTestId("delivery-undo")).toContainText("Undo the changes to 1 file(s)?");
  await page.getByTestId("undo-run-files-confirm").click();
  await expect(page.getByTestId("undo-run-files-result")).toHaveText(
    "Restored 1, removed 1; 0 left alone because they changed since.",
  );
  const state = await readMockState<{ undoCalls?: { runId: string }[] }>(page);
  expect(state.undoCalls).toEqual([expect.objectContaining({ runId: "run-7" })]);
});

test("declining the question changes nothing", async ({ page }) => {
  await overrideMockCommand(page, "session_messages", MESSAGES);
  await page.goto("/");
  await page.getByText("Draft the launch note").first().click();
  await page.getByTestId("undo-run-files").click();
  await page.getByTestId("undo-run-files-cancel").click();
  await expect(page.getByTestId("undo-run-files")).toBeVisible();
  const state = await readMockState<{ undoCalls?: unknown[] }>(page);
  expect(state.undoCalls ?? []).toEqual([]);
});
