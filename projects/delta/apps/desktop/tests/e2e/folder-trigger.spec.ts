import { expect } from "@playwright/test";
import { overrideMockCommand, readMockState, test } from "./fixtures";

// An automation can start when a file arrives in a folder instead of at a time. The chosen folder
// becomes its workspace; the list and the detail say what starts it in words.

const CREATE = `(args) => {
  const b = args.payload || {};
  $state.createdAutomations = ($state.createdAutomations || []).concat([b]);
  const task = {
    ...$state.automations[0], id: "task-folder", title: b.title, instructions: b.instructions,
    workspace: b.workspace || "", schedule: b.watch_folder ? "folder:" + b.watch_folder : b.cron,
    schedule_raw: b.watch_folder
      ? { kind: "folder", cron: null, fire_at: null, folder: b.watch_folder, pattern: b.watch_pattern || null }
      : { kind: "cron", cron: b.cron },
    next_run: null, run_count: 0, always_allowed: [],
  };
  $state.automations.push(task);
  return { ok: true, task };
}`;

async function openAutomations(page: import("@playwright/test").Page) {
  await page.goto("/");
  await page.getByTestId("nav-automations").click();
  await expect(page.getByText("Recurring tasks Delta runs on a schedule.")).toBeVisible();
}

test("a folder trigger is created from the chosen folder and shown in words", async ({ page }) => {
  await overrideMockCommand(page, "automation_create", CREATE);
  await openAutomations(page);
  await page.getByRole("button", { name: /New automation/ }).first().click();

  await page.getByPlaceholder(/^Title/).fill("Sort the inbox");
  await page.locator("textarea.tmpl-textarea").fill("Summarize each new file.");
  const create = page.getByRole("button", { name: /Create automation/ });
  await page.getByTestId("trigger-kind").selectOption("folder");
  // A folder is needed before it can be created.
  await expect(create).toBeDisabled();
  await page.getByTestId("choose-watch-folder").click();
  await expect(page.getByTestId("choose-watch-folder")).toHaveText("picked-folder");
  await page.getByTestId("watch-pattern").fill("*.pdf");
  await expect(create).toBeEnabled();
  await create.click();

  const state = await readMockState<{ createdAutomations?: Record<string, unknown>[] }>(page);
  expect(state.createdAutomations?.[0]).toMatchObject({
    title: "Sort the inbox", workspace: "/tmp/picked-folder", watch_folder: ".", watch_pattern: "*.pdf",
  });
  expect(state.createdAutomations?.[0]?.cron).toBeUndefined();

  // The detail says what starts it and that it is watching; there is no time to edit.
  await expect(page.getByText("When *.pdf arrives in picked-folder").first()).toBeVisible();
  await expect(page.getByText(/Watching/)).toBeVisible();
  await expect(page.getByText(/Active · next/)).toHaveCount(0);
});

test("a time trigger is still the default", async ({ page }) => {
  await overrideMockCommand(page, "automation_create", CREATE);
  await openAutomations(page);
  await page.getByRole("button", { name: /New automation/ }).first().click();
  await expect(page.getByTestId("trigger-kind")).toHaveValue("time");
  await expect(page.getByTestId("choose-watch-folder")).toHaveCount(0);
});
