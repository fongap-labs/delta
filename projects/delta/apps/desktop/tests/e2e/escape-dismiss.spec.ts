import { expect } from "@playwright/test";
import { overrideMockCommand, test } from "./fixtures";

// Escape closes the layer that owns it: an open row menu, or an open artifact (back to the list).
// It is left alone while an input method is composing text and while typing in a field.

test("Escape closes an open row menu, but not while an input method is composing", async ({ page }) => {
  await page.goto("/");
  const row = page.getByTitle("Weekly plan 2");
  await expect(row).toBeVisible();
  await row.hover();
  await row.getByTestId("row-menu").click();
  await expect(row.getByTestId("row-menu-pin")).toBeVisible();

  // Escape that belongs to an IME composition must not close the menu.
  await page.evaluate(() =>
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", isComposing: true, bubbles: true, cancelable: true })),
  );
  await expect(row.getByTestId("row-menu-pin")).toBeVisible();

  await page.keyboard.press("Escape");
  await expect(row.getByTestId("row-menu-pin")).toHaveCount(0);
});

test("Escape steps back from an open artifact, but not while typing in the composer", async ({ page }) => {
  await overrideMockCommand(
    page,
    "artifacts_list",
    `(args) => ({ artifacts: [{ path: "notes/plain.md", name: "plain.md", kind: "markdown", size: 40, modified_at: 1760000200 }] })`,
  );
  await overrideMockCommand(page, "artifact_read", `(args) => ({ ok: true, path: args.path, kind: "markdown", content: "# " + args.path })`);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.getByText("Draft the launch note").first().click();
  await page.getByRole("button", { name: /plain\.md/ }).first().click();
  const heading = page.getByRole("heading", { name: "notes/plain.md" });
  await expect(heading).toBeVisible();

  // Typing in the composer: Escape is the field's, the artifact stays open.
  const box = page.getByPlaceholder(/Ask Delta/);
  await box.click();
  await page.keyboard.press("Escape");
  await expect(heading).toBeVisible();

  // Focus elsewhere: Escape goes back to the artifact list.
  await heading.click();
  await page.keyboard.press("Escape");
  await expect(heading).toHaveCount(0);
  await expect(page.getByRole("button", { name: /plain\.md/ }).first()).toBeVisible();
});
