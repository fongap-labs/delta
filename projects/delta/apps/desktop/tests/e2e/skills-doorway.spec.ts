import { expect, test } from "./fixtures";

// SKILLS-SPEC §5.2: creating a skill is a conversation. "Create with Delta" opens a fresh
// session with the opening sentence in the composer; the user finishes it and sends. Nothing
// is sent or saved until they do, and saving still goes through the save_skill approval.

test("skills-doorway: Create with Delta opens a new conversation with the sentence prefilled", async ({ page }) => {
  await page.goto("/");
  await page.getByTestId("sidebar-footer-settings").click();
  await page.getByRole("button", { name: "Skills", exact: true }).click();
  await page.getByRole("button", { name: /Add skill/ }).click();
  await page.getByText("Create with Delta").click();

  const composer = page.getByPlaceholder(/Ask Delta/);
  await expect(composer).toHaveValue(/Build a new skill for me:/);
  // The user finishes the sentence; the prefill is editable text, not a sent message.
  await composer.fill("Build a new skill for me: weekly report generator");
  await expect(composer).toHaveValue("Build a new skill for me: weekly report generator");
});
