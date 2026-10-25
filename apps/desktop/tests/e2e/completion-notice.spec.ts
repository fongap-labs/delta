// A finished automation leaves an Inbox notice that links to the files it made; a click opens the
// run's session and the file in it.
import { expect } from "@playwright/test";
import { overrideMockCommand, patchMockState, test } from "./fixtures";

const RUN_SESSION = "__run__abc123";

const NOTICE = {
  id: "done-1",
  session_id: RUN_SESSION,
  kind: "notice",
  title: "Finished: Weekly report",
  body: "All done. The report is ready.",
  state: "pending",
  resolution: null,
  inbox: "default",
  created_at: "2026-10-25 08:00:00",
  resolved_at: null,
  data: { completion: true, run_id: "abc123", artifacts: [{ path: "reports/final.md", name: "final.md" }] },
  session_title: "Weekly report",
  session_agent: "delta",
  session_workspace: "",
  session_exists: true,
};

test.beforeEach(async ({ page }) => {
  await patchMockState(page, {
    inbox: [NOTICE],
    sessions: [
      { session_id: RUN_SESSION, title: "Weekly report", workspace: "", agent: "delta", n_msgs: 2, updated_at: "2026-10-25 08:00:00" },
    ],
    sessionMessages: { [RUN_SESSION]: [{ role: "user", content: "Write it" }, { role: "assistant", content: "All done." }] },
  });
  await overrideMockCommand(
    page,
    "artifacts_list",
    `(args) => ({ artifacts: [{ path: "reports/final.md", name: "final.md", kind: "markdown", size: 20, modified_at: 1760000000 }] })`,
  );
  await overrideMockCommand(page, "artifact_read", `(args) => ({ ok: true, path: args.path, kind: "markdown", content: "# " + args.path })`);
});

test("the notice shows the summary and one button per file", async ({ page }) => {
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.getByRole("button", { name: /Inbox/ }).first().click();
  await expect(page.getByText("Finished: Weekly report")).toBeVisible();
  await expect(page.getByText("All done. The report is ready.")).toBeVisible();
  await expect(page.getByTestId("inbox-artifact")).toHaveCount(1);
  await expect(page.getByTestId("inbox-artifact")).toContainText("Open final.md");
});

test("a file button opens the run's session with the file in the viewer", async ({ page }) => {
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.getByRole("button", { name: /Inbox/ }).first().click();
  await page.getByTestId("inbox-artifact").click();
  await expect(page.getByRole("heading", { name: "reports/final.md" })).toBeVisible();
});

test("dismissing still works beside the file buttons", async ({ page }) => {
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.getByRole("button", { name: /Inbox/ }).first().click();
  await page.getByRole("button", { name: "Dismiss" }).click();
  await expect(page.getByText("Finished: Weekly report")).toHaveCount(0);
});
