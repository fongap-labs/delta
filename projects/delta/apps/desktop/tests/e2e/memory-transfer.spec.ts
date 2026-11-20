import { readFile } from "node:fs/promises";
import { expect, overrideMockCommand, readMockState, test } from "./fixtures";

const ONE_MEMORY = `(args) => ({ memory: [{ id: 1, scope: "global", content: "Prefers short answers", summary: "Prefers short answers", created_at: "2026-10-24 09:00:00" }] })`;

async function openMemory(page: import("@playwright/test").Page) {
  await overrideMockCommand(page, "memory_list", ONE_MEMORY);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await expect(page.getByPlaceholder(/Ask Delta/)).toBeVisible();
  await page.getByRole("button", { name: /Settings/ }).first().click();
  await page.getByRole("button", { name: /^Memory/ }).first().click();
  await expect(page.getByTestId("memory-list-card")).toBeVisible();
}

test("Memory: Export downloads the memories as a delta.memory file", async ({ page }) => {
  await openMemory(page);
  const download = page.waitForEvent("download");
  await page.getByTestId("memory-export").click();
  const file = await download;
  expect(file.suggestedFilename()).toBe("delta-memory.json");
  const exported = JSON.parse(await readFile((await file.path())!, "utf8"));
  expect(exported.format).toBe("delta.memory");
  expect(exported.version).toBe(1);
  expect(exported.memories[0].content).toBe("Prefers short answers");
});

test("Memory: Import sends the chosen file to the runtime and says what it added", async ({ page }) => {
  await openMemory(page);
  const document = { format: "delta.memory", version: 1, memories: [{ scope: "global", content: "Likes tables" }] };
  await page.getByTestId("memory-import-input").setInputFiles({
    name: "memory.json",
    mimeType: "application/json",
    buffer: Buffer.from(JSON.stringify(document)),
  });
  await expect(page.getByTestId("memory-list-msg")).toContainText("Imported 1 memories");
  const state = await readMockState<{ memoryImports?: unknown[] }>(page);
  expect(state.memoryImports).toEqual([document]);
});

test("Memory: Import of a file that is not JSON says so and sends nothing", async ({ page }) => {
  await openMemory(page);
  await page.getByTestId("memory-import-input").setInputFiles({
    name: "memory.json",
    mimeType: "application/json",
    buffer: Buffer.from("not json"),
  });
  await expect(page.getByTestId("memory-list-msg")).toContainText(/import that file/);
  const state = await readMockState<{ memoryImports?: unknown[] }>(page);
  expect(state.memoryImports ?? []).toEqual([]);
});
