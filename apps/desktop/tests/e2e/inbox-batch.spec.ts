// The inbox groups pending items by task and answers a task's approvals in one step.
import { expect } from "@playwright/test";
import { patchMockState, readMockState, test } from "./fixtures";

const approval = (id: string, session: string, title: string, tool: string) => ({
  id,
  session_id: session,
  kind: "approval",
  title,
  body: "needs approval",
  state: "pending",
  resolution: null,
  inbox: "default",
  created_at: "2026-07-01 08:00:00",
  resolved_at: null,
  data: { tool, arguments: {} },
  session_title: session === "task-a" ? "Weekly report" : "Cleanup",
  session_agent: "delta",
  session_workspace: "",
  session_exists: true,
});

const ITEMS = [
  approval("a1", "task-a", "Run write_file?", "write_file"),
  approval("a2", "task-a", "Run web_fetch?", "web_fetch"),
  approval("b1", "task-b", "Run write_file?", "write_file"),
];

async function openInbox(page) {
  await patchMockState(page, { inbox: ITEMS });
  await page.goto("/");
  await page.getByRole("button", { name: /Inbox/ }).first().click();
  await expect(page.getByTestId("inbox-group")).toHaveCount(2);
}

const resolutions = async (page) => {
  const state = await readMockState<{ inbox: { id: string; state: string; resolution: string | null }[] }>(page);
  return Object.fromEntries(state.inbox.map((item) => [item.id, item.resolution]));
};

test("a task with several waiting items gets a header; a lone item does not", async ({ page }) => {
  await openInbox(page);
  await expect(page.getByTestId("inbox-group-head")).toHaveCount(1);
  const head = page.getByTestId("inbox-group-head");
  await expect(head).toContainText("Weekly report");
  await expect(head).toContainText("2 waiting");
  await expect(page.getByTestId("inbox-allow-all")).toContainText("Allow all 2");
  await expect(page.getByTestId("inbox-deny-all")).toContainText("Deny all 2");
});

test("Deny all answers every approval of that task and leaves the others", async ({ page }) => {
  await openInbox(page);
  await page.getByTestId("inbox-deny-all").click();
  await expect.poll(async () => (await resolutions(page)).a1).toBe("deny");
  expect(await resolutions(page)).toEqual({ a1: "deny", a2: "deny", b1: null });
});

test("Allow all names what it allows and does nothing when declined", async ({ page }) => {
  await openInbox(page);
  let message = "";
  page.once("dialog", async (dialog) => {
    message = dialog.message();
    await dialog.dismiss();
  });
  await page.getByTestId("inbox-allow-all").click();
  await expect.poll(() => message).toContain("write_file, web_fetch");
  expect(await resolutions(page)).toEqual({ a1: null, a2: null, b1: null });

  page.once("dialog", (dialog) => void dialog.accept());
  await page.getByTestId("inbox-allow-all").click();
  await expect.poll(async () => (await resolutions(page)).a1).toBe("allow");
  expect(await resolutions(page)).toEqual({ a1: "allow", a2: "allow", b1: null });
});
