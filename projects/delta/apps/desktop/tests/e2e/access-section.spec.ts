// The rail's Access sources and folders (§32). They sit inside the rail's Permissions section,
// which is open by default, so there is no separate Access header to expand. Fixture state:
// browser + slack + github connected/enabled (github is two_way WITHOUT channels — relay
// mentions, no subscriptions), gmail recommended-not-connected, one primary root.
import { expect } from "@playwright/test";
import { test } from "./fixtures";

test("Access lives inside Permissions; sources and folders render inline", async ({ page }) => {
  await page.goto("/");
  await page.getByText("Draft the launch note").first().click();

  // §32: the settings row/icon is gone from the topbar — the panel toggle is the one entry.
  await expect(page.getByRole("button", { name: "Open session settings" })).toHaveCount(0);
  await expect(page.getByTestId("session-settings-row")).toHaveCount(0);

  // Sources (per-session toggles), Recommended (with its reason), Folders — all inline in the
  // Permissions section; no dialog appears anywhere.
  const section = page.getByTestId("access-section");
  await expect(section.locator("xpath=ancestor::section[1]").locator(".rail-section-toggle")).toHaveText("Permissions");
  const body = page.getByRole("region", { name: "Session access" });
  await expect(body.getByText("Connectors", { exact: true })).toBeVisible();
  await expect(body.getByText("Slack", { exact: true })).toBeVisible();
  await expect(body.getByText("email context for morning summaries")).toBeVisible();
  await expect(body.getByTestId("drawer-directories").getByText("Temporary space")).toBeVisible();
  await expect(page.getByRole("dialog")).toHaveCount(0);

  // Channels is a chat capability, not a two_way one: Slack gets the drill-down, GitHub
  // (two_way via the relay, no channel semantics) must NOT (owner report 2026-07-13).
  await expect(body.getByRole("button", { name: /Channels ·/ })).toHaveCount(1);
  await expect(body.getByText("GitHub", { exact: true })).toBeVisible();
});

test("+ Add a connector: full catalog on focus, filter as you type → connect-in-context; connected sources never match", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByText("Draft the launch note").first().click();

  // Focusing the empty input shows the FULL catalog (FB-012) — every available connector
  // minus the already-connected three, before any typing.
  await page.getByTestId("access-add-source").click();
  const search = page.getByTestId("access-add-search");
  await expect(search).toBeFocused();
  const rows = page.locator('[data-testid^="access-add-"]:not([data-testid="access-add-search"])');
  await expect(rows).toHaveCount(9); // 12 in the catalog − browser/slack/github (connected)
  await expect(page.getByTestId("access-add-notion")).toBeVisible();

  // Already-connected sources don't match (Slack and GitHub are connected in fixtures)…
  await search.fill("slack");
  await expect(page.getByText("No match — see all on the Connectors page below.")).toBeVisible();
  await search.fill("github");
  await expect(page.getByText("No match — see all on the Connectors page below.")).toBeVisible();

  // …and clearing the query restores the full list ("filter as you type", not search-only).
  await search.fill("");
  await expect(rows).toHaveCount(9);

  // Capability aliases match too: "calendar" surfaces Outlook (title alone never would).
  await search.fill("calendar");
  await expect(page.getByTestId("access-add-outlook")).toBeVisible();

  // …the long tail does: Notion is in the catalog but neither connected nor recommended.
  await search.fill("notion");
  await page.getByTestId("access-add-notion").click();

  // Lands in the SAME connect-in-context child view the Recommended flow uses, with the
  // scope-semantics line; back returns to the Sources list.
  const body = page.getByRole("region", { name: "Session access" });
  await expect(body.getByText("Connecting makes Notion available to all your Delta agents", { exact: false })).toBeVisible();
  await expect(body.getByPlaceholder("ntn_…")).toBeVisible();
  await body.getByRole("button", { name: "Back to connectors" }).click();
  await expect(body.getByText("Slack", { exact: true })).toBeVisible();
});

test("per-session mute round-trips", async ({ page }) => {
  await page.goto("/");
  await page.getByText("Draft the launch note").first().click();

  const body = page.getByRole("region", { name: "Session access" });
  // Muting Slack for this session turns its switch off (the fixture flips enabled on POST and
  // the section reloads).
  const slackSwitch = body.getByRole("switch").nth(1);
  await slackSwitch.click();
  await expect(slackSwitch).toHaveAttribute("aria-checked", "false");
});
