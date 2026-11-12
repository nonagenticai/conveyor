import { expect, test } from "@playwright/test";

// The palette is the guarantee that "simple" never means "you cannot do the thing": it is
// the same role-filtered manifest the server serves, and the payload is the action's input.
test("⌘K finds an action and invokes it", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Today" })).toBeVisible();

  await page.keyboard.press("ControlOrMeta+k");
  const palette = page.locator(".palette");
  await expect(palette).toBeVisible();

  await page.locator(".palette-input").fill("tenants.get");
  await palette.getByRole("button", { name: "tenants.get" }).click();
  await expect(palette.locator("textarea")).toBeVisible();
  await page.getByRole("button", { name: "Invoke" }).click();
  await expect(page.locator(".palette-result")).toContainText("ok");

  await page.keyboard.press("Escape");
  await expect(palette).toBeHidden();
});
