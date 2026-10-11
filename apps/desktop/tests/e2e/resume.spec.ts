import { expect } from "@playwright/test";
import { overrideMockCommand, patchMockState, test } from "./fixtures";

// After an interruption the runtime says whether it can resume; only then the notice offers
// Resume. The UI never decides that from what it sees.

const SESSION = "pinned-delta-1";
const INTERRUPTED = [
  { role: "user", content: "build the report" },
  { role: "notice", kind: "interrupted" },
];

const RESUMABLE = `(args) => ({ resumable: $state.resumable === true, runId: "run-1" })`;
const RESUME = `(args) => {
  const messages = ($state.sessionMessages[args.sessionId] = $state.sessionMessages[args.sessionId] || []);
  $state.resumable = false;
  $state.resumeCalls = ($state.resumeCalls || 0) + 1;
  const send = (type, payload) => {
    const body = payload || {};
    if (type === "assistant_message") messages.push({ role: "assistant", content: String(body.text || ""), ts: Date.now() / 1000 });
    const sequence = ($state.runtimeSequences[args.sessionId] = ($state.runtimeSequences[args.sessionId] || 1) + 1);
    emit("delta-runtime-event", { type, version: 1, sessionId: args.sessionId, sequence, payload: body });
  };
  send("turn_start", { input: "(resumed)" });
  send("assistant_message", { text: "Picked up where it stopped." });
  send("turn_done");
  return { ok: true, accepted: true, runId: "run-1" };
}`;

async function open(page: import("@playwright/test").Page, isResumable: boolean) {
  await patchMockState(page, { sessionMessages: { [SESSION]: INTERRUPTED }, resumable: isResumable });
  await overrideMockCommand(page, "runtime_resumable", RESUMABLE);
  await overrideMockCommand(page, "runtime_resume", RESUME);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.getByText("Draft the launch note").first().click();
}

test("an interrupted run the runtime can resume offers Resume, and Resume continues it", async ({ page }) => {
  await open(page, true);
  await expect(page.getByText("Interrupted.")).toBeVisible();
  const resume = page.getByTestId("notice-resume");
  await expect(resume).toBeVisible();
  await resume.click();
  await expect(page.getByText("Picked up where it stopped.").first()).toBeVisible({ timeout: 10_000 });
  await expect(page.getByTestId("notice-resume")).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).__DELTA_MOCK__.state.resumeCalls)).toBe(1);
});

test("no Resume when the runtime says there is nothing to resume", async ({ page }) => {
  await open(page, false);
  await expect(page.getByText("Interrupted.")).toBeVisible();
  await expect(page.getByTestId("notice-resume")).toHaveCount(0);
});

test("no Resume when asking the runtime fails", async ({ page }) => {
  await open(page, true);
  await overrideMockCommand(page, "runtime_resumable", `(args) => { throw new Error("boom"); }`);
  await page.reload({ waitUntil: "domcontentloaded" });
  await page.getByText("Draft the launch note").first().click();
  await expect(page.getByText("Interrupted.")).toBeVisible();
  await expect(page.getByTestId("notice-resume")).toHaveCount(0);
});
