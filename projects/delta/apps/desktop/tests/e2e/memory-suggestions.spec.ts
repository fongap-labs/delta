import { expect, type Page } from "@playwright/test";
import { overrideMockCommand, patchMockState, readMockState, test } from "./fixtures";

// Delta only suggests what to remember: a suggestion waits, in Settings ▸ Memory and in the
// conversation, until the user accepts, edits or declines it.

const CANDIDATES = [
  {
    id: 7,
    content: "Writes reports in Chinese",
    source: { kind: "session", session_id: "s1", run_id: "r1", quote: "I always write in Chinese" },
    created_at: "2026-10-28 09:00:00",
  },
  { id: 8, content: "Prefers metric units", source: null, created_at: "2026-10-28 09:05:00" },
];

async function openMemory(page: Page) {
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await expect(page.getByPlaceholder(/Ask Delta/)).toBeVisible();
  await page.getByRole("button", { name: /Settings/ }).first().click();
  await page.getByRole("button", { name: /^Memory/ }).first().click();
  await expect(page.getByTestId("memory-list-card")).toBeVisible();
}

test("memory starts off, and switching it on says nothing is saved without the user", async ({ page }) => {
  await openMemory(page);
  const toggle = page.getByTestId("memory-toggle-card").getByRole("switch");
  await expect(toggle).toHaveAttribute("aria-checked", "false");
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-checked", "true");
  await expect(page.getByTestId("memory-toggle-msg")).toContainText("Nothing is saved until you accept it");
  expect((await readMockState<{ memoryEnabled?: boolean }>(page)).memoryEnabled).toBe(true);
});

test("a waiting suggestion can be accepted, edited into the user's own words, or declined", async ({ page }) => {
  await patchMockState(page, { memoryEnabled: true, memoryCandidates: CANDIDATES });
  await openMemory(page);
  const card = page.getByTestId("memory-suggestions-card");
  await expect(card).toContainText("Writes reports in Chinese");
  await expect(card).toContainText("You said: “I always write in Chinese”");

  await page.getByTestId("memory-candidate-accept-7").click();
  await expect(page.getByTestId("memory-candidate-7")).toHaveCount(0);

  await page.getByTestId("memory-candidate-edit-8").click();
  await page.getByTestId("memory-candidate-input-8").fill("Prefers metric units, except for screens");
  await page.getByTestId("memory-candidate-save-8").click();
  await expect(page.getByTestId("memory-suggestions-card")).toHaveCount(0);

  const state = await readMockState<{ memoryAccepts?: { id: number; content: string | null }[] }>(page);
  expect(state.memoryAccepts).toEqual([
    { id: 7, content: null },
    { id: 8, content: "Prefers metric units, except for screens" },
  ]);
});

test("declining a suggestion tells the runtime so it is not offered again", async ({ page }) => {
  await patchMockState(page, { memoryEnabled: true, memoryCandidates: CANDIDATES });
  await openMemory(page);
  await page.getByTestId("memory-candidate-decline-8").click();
  await expect(page.getByTestId("memory-candidate-8")).toHaveCount(0);
  expect((await readMockState<{ memoryRejects?: number[] }>(page)).memoryRejects).toEqual([8]);
});

const PROPOSAL_MESSAGES = `(args) => ({ messages: [
  { role: "user", content: "I always write in Chinese, please keep that in mind", ts: 1760000000 },
  { role: "assistant", content: "", tool_calls: [{ id: "m1", function: { name: "memory_propose", arguments: JSON.stringify({ content: "Writes reports in Chinese", evidence: "I always write in Chinese" }) } }] },
  { role: "tool", tool_call_id: "m1", content: JSON.stringify({ ok: true, candidate_id: 7 }) },
  { role: "assistant", content: "Noted — I'll write in Chinese.", ts: 1760000005 }
] })`;

async function openProposalConversation(page: Page) {
  await overrideMockCommand(page, "session_messages", PROPOSAL_MESSAGES);
  await page.goto("/");
  await page.getByText("Draft the launch note").first().click();
}

test("the conversation asks before remembering, and an accepted note can be undone", async ({ page }) => {
  await patchMockState(page, { memoryEnabled: true, memoryCandidates: CANDIDATES });
  await openProposalConversation(page);
  const card = page.getByTestId("memory-suggestion");
  await expect(card).toContainText("Remember this for next time?");
  await expect(card).toContainText("Writes reports in Chinese");
  await expect(page.getByTestId("memory-suggestion-quote")).toContainText("I always write in Chinese");

  await page.getByTestId("memory-suggestion-accept").click();
  await expect(card).toContainText("Remembered");
  expect((await readMockState<{ memoryAccepts?: { id: number }[] }>(page)).memoryAccepts).toEqual([
    { id: 7, content: null },
  ]);

  await page.getByTestId("memory-suggestion-undo").click();
  await expect(page.getByTestId("memory-suggestion-undone")).toHaveText("Okay — forgotten.");
});

test("a suggestion can be edited in the conversation before it is saved", async ({ page }) => {
  await patchMockState(page, { memoryEnabled: true, memoryCandidates: CANDIDATES });
  await openProposalConversation(page);
  await page.getByTestId("memory-suggestion-edit").click();
  await page.getByTestId("memory-suggestion-input").fill("Always writes reports in Chinese");
  await page.getByTestId("memory-suggestion-save").click();
  await expect(page.getByTestId("memory-suggestion")).toContainText("Always writes reports in Chinese");
  expect((await readMockState<{ memoryAccepts?: { content: string }[] }>(page)).memoryAccepts).toEqual([
    { id: 7, content: "Always writes reports in Chinese" },
  ]);
});

test("declining in the conversation is final", async ({ page }) => {
  await patchMockState(page, { memoryEnabled: true, memoryCandidates: CANDIDATES });
  await openProposalConversation(page);
  await page.getByTestId("memory-suggestion-decline").click();
  await expect(page.getByTestId("memory-suggestion-declined")).toContainText("I won’t remember that");
  expect((await readMockState<{ memoryRejects?: number[] }>(page)).memoryRejects).toEqual([7]);
});

test("a suggestion already decided elsewhere is not offered a second time", async ({ page }) => {
  await patchMockState(page, { memoryEnabled: true, memoryCandidates: [] });
  await openProposalConversation(page);
  await expect(page.getByText("Noted — I'll write in Chinese.")).toBeVisible();
  await expect(page.getByTestId("memory-suggestion")).toHaveCount(0);
});
