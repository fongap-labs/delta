import { expect } from "@playwright/test";
import { overrideMockCommand, patchMockState, test } from "./fixtures";

// The artifact viewer shows what the runtime recorded on the tool result that produced the file:
// validation verdict, producer and checksum. Files with no such record show no strip at all.

const SESSION = "pinned-delta-1";

const ARTIFACTS = [
  { path: "reports/q3.md", name: "q3.md", kind: "markdown", size: 120, modified_at: 1760000000 },
  { path: "reports/draft.md", name: "draft.md", kind: "markdown", size: 80, modified_at: 1760000100 },
  { path: "notes/plain.md", name: "plain.md", kind: "markdown", size: 40, modified_at: 1760000200 },
];

const MESSAGES = [
  { role: "user", content: "make the reports" },
  { role: "assistant", content: "", tool_calls: [{ id: "call-a", type: "function", function: { name: "make_report", arguments: "{}" } }] },
  {
    role: "tool",
    tool_call_id: "call-a",
    content: {
      ok: true,
      artifacts: [{ path: "reports/q3.md", sha256: "ab12cd34ef567890", run_id: "run-1" }],
      validation: { ok: true, checks: [{ name: "artifact_count", ok: true, detail: "1 artifact" }, { name: "all_artifacts_complete", ok: true, detail: "" }] },
    },
  },
  { role: "assistant", content: "", tool_calls: [{ id: "call-b", type: "function", function: { name: "make_report" , arguments: "{}" } }] },
  {
    role: "tool",
    tool_call_id: "call-b",
    content: JSON.stringify({
      ok: true,
      artifacts: [{ path: "reports/draft.md", sha256: "9988776655443322", run_id: "run-1" }],
      validation: { ok: false, checks: [{ name: "required_substrings", ok: false, detail: "missing: Summary" }, { name: "artifact_count", ok: true, detail: "" }] },
    }),
  },
  { role: "assistant", content: "Done." },
];

test.beforeEach(async ({ page }) => {
  await patchMockState(page, { sessionMessages: { [SESSION]: MESSAGES } });
  await overrideMockCommand(page, "artifacts_list", `(args) => ({ artifacts: ${JSON.stringify(ARTIFACTS)} })`);
  await overrideMockCommand(page, "artifact_read", `(args) => ({ ok: true, path: args.path, kind: "markdown", content: "# " + args.path })`);
});

async function openArtifact(page: import("@playwright/test").Page, name: string) {
  // The artifact rail keeps a sub-resource pending in the test browser, so `load` never fires.
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.getByText("Draft the launch note").first().click();
  await page.getByRole("button", { name: new RegExp(name) }).first().click();
}

test("a validated artifact shows the verdict, producer and checksum, and lists its checks", async ({ page }) => {
  await openArtifact(page, "q3.md");
  const strip = page.getByTestId("trust-strip");
  await expect(strip).toBeVisible();
  await expect(page.getByTestId("trust-verdict")).toContainText("2 of 2 checks passed");
  await expect(page.getByTestId("trust-verdict")).toHaveAttribute("data-verdict", "ok");
  await expect(page.getByTestId("trust-producer")).toContainText("Made by make_report");
  await expect(page.getByTestId("trust-checksum")).toContainText("SHA-256 ab12cd34");

  await expect(page.getByTestId("trust-checks")).toHaveCount(0);
  await page.getByTestId("trust-verdict").click();
  await expect(page.getByTestId("trust-checks")).toContainText("artifact_count");
});

test("a failed check is shown as failed, with the reason", async ({ page }) => {
  await openArtifact(page, "draft.md");
  await expect(page.getByTestId("trust-verdict")).toContainText("1 of 2 checks failed");
  await expect(page.getByTestId("trust-verdict")).toHaveAttribute("data-verdict", "failed");
  await page.getByTestId("trust-verdict").click();
  await expect(page.getByTestId("trust-checks")).toContainText("missing: Summary");
});

test("an artifact no tool result registered shows no trust strip", async ({ page }) => {
  await openArtifact(page, "plain.md");
  await expect(page.getByRole("heading", { name: "notes/plain.md" })).toBeVisible();
  await expect(page.getByTestId("trust-strip")).toHaveCount(0);
});
