import { expect } from "@playwright/test";
import { test } from "./fixtures";

// Render real pages with mocked Runtime transport; capture both theme variants and
// guard computed styles + page containment (not pixel-diff or WCAG contrast certification).
for (const mode of ["light", "dark"] as const) {
  test(`settings page follows ${mode} palette and stays contained`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width: 1180, height: 800 });
    await page.addInitScript((theme) => {
      localStorage.setItem("delta-theme", theme);
    }, mode);
    await page.goto("/");
    await expect(page.locator(".app")).not.toHaveClass(/boot-splash/);
    await expect(page.locator("html")).toHaveAttribute("data-theme", mode);
    await page.getByTestId("sidebar-footer-settings").click();
    await expect(page.getByRole("heading", { name: "General" })).toBeVisible();

    const colors = await page.evaluate(() => {
      const root = document.documentElement;
      const css = getComputedStyle(root);
      const probe = document.createElement("button");
      probe.className = "ds-button ds-button-primary";
      probe.textContent = "Theme probe";
      document.body.append(probe);
      const buttonCss = getComputedStyle(probe);
      const values = {
        primary: css.getPropertyValue("--primary").trim(),
        buttonBackground: buttonCss.backgroundColor,
        buttonForeground: buttonCss.color,
        viewportWidth: root.clientWidth,
        documentWidth: root.scrollWidth,
      };
      probe.remove();
      return values;
    });

    expect(colors.primary).toBe(mode === "light" ? "#286f78" : "#60b4bf");
    expect(colors.buttonBackground).toBe(mode === "light" ? "rgb(40, 111, 120)" : "rgb(96, 180, 191)");
    expect(colors.buttonForeground).toBe(mode === "light" ? "rgb(255, 255, 255)" : "rgb(14, 26, 28)");
    expect(colors.documentWidth).toBeLessThanOrEqual(colors.viewportWidth + 1);

    await testInfo.attach(`settings-${mode}.png`, {
      body: await page.screenshot({ fullPage: true }),
      contentType: "image/png",
    });
  });
}

test("settings remains within the window at compact desktop width", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1024, height: 720 });
  await page.goto("/");
  await expect(page.locator(".app")).not.toHaveClass(/boot-splash/);
  await page.getByTestId("sidebar-footer-settings").click();
  await expect(page.getByRole("heading", { name: "General" })).toBeVisible();
  const widths = await page.evaluate(() => ({
    document: document.documentElement.scrollWidth,
    viewport: document.documentElement.clientWidth,
  }));
  expect(widths.document).toBeLessThanOrEqual(widths.viewport + 1);
  await testInfo.attach("settings-compact.png", {
    body: await page.screenshot({ fullPage: true }),
    contentType: "image/png",
  });
});
