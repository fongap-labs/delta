// Workspace-chat detail is rendered by a generic Foundation capability page.
// The fixture still uses Slack as the concrete catalog entry, but the detail
// contract must not depend on vendor-specific DOM ids or UI implementation.
import { expect } from "@playwright/test";
import { test } from "./fixtures";

async function openWorkspaceChatPage(page) {
  await page.goto("/");
  await page.getByTestId("sidebar-footer-integrations").click();
  await page.getByTestId("connector-slack").click();
}

test("workspace-chat row navigates to the generic detail page", async ({ page }) => {
  await page.goto("/");
  await page.getByTestId("sidebar-footer-integrations").click();

  const row = page.getByTestId("connector-slack");
  await expect(row).toContainText("Slack");
  await row.click();
  await expect(page.getByTestId("workspace-chat-detail")).toBeVisible();
  await expect(page.getByTestId("workspace-chat-workspace-T1DL")).toBeVisible();
});

test("parked sender files under its workspace; Allow & deliver adds to that allow-list", async ({
  page,
}) => {
  await openWorkspaceChatPage(page);

  const workspace = page.getByTestId("workspace-chat-workspace-T1DL");
  await expect(workspace.getByTestId("waiting-pk1")).toContainText("Maya");
  await expect(workspace.getByTestId("waiting-pk1")).toContainText("in #delta-test");
  await expect(workspace.getByTestId("waiting-pk1")).toContainText(
    "hey delta, can you summarize this thread?",
  );

  await page.getByTestId("workspace-chat-allow-deliver-pk1").click();
  await expect(page.getByTestId("waiting-pk1")).toHaveCount(0);
  await expect(workspace).toContainText("U0NEW");
});

test("parked sender can be dismissed without allowing", async ({ page }) => {
  await openWorkspaceChatPage(page);
  await page.getByTestId("workspace-chat-dismiss-pk1").click();
  await expect(page.getByTestId("waiting-pk1")).toHaveCount(0);
  await expect(page.getByTestId("workspace-chat-workspace-T1DL")).not.toContainText("U0NEW");
});

test("sessions listening in the workspace are listed with unsubscribe", async ({ page }) => {
  await openWorkspaceChatPage(page);

  const workspace = page.getByTestId("workspace-chat-workspace-T1DL");
  const listening = workspace.getByTestId("workspace-chat-listening");
  await expect(listening).toContainText("Weekly plan 1");
  await expect(listening).toContainText("#delta-test");

  await listening.getByTitle("Unsubscribe this session").click();
  await expect(workspace.getByTestId("workspace-chat-listening")).toHaveCount(0);
});
