import { expect, test } from "@playwright/test";

// One switch, not a panel per screen: the same root attribute deepens every wireframe.
test("Advanced deepens the screen and toggles back", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Today" })).toBeVisible();

  const advanced = page.locator(".adv").first();
  await expect(advanced).toBeHidden();

  const toggle = page.getByRole("button", { name: /Advanced/ });
  await toggle.click();
  await expect(advanced).toBeVisible();
  await expect(toggle).toHaveAttribute("aria-pressed", "true");

  await toggle.click();
  await expect(advanced).toBeHidden();
  await expect(toggle).toHaveAttribute("aria-pressed", "false");
});
