import { expect, test } from "./fixture";

// T18-3 P2: the Authoring destination is T17's, and no `authoring.*` action is registered yet.
// The route stays honest: it renders the gate naming the owning slice, with no write control and
// no fixture row. When T17 lands, the live screen replaces this gate and this spec.

test("the destination is not yet live and names its owning slice", async ({ page }) => {
  await page.goto("/authoring");
  await expect(page.getByRole("heading", { name: "Authoring" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Not yet live" })).toBeVisible();
  await expect(page.getByText(/owner: T17/)).toBeVisible();
  await expect(page.getByText(/screen and actions are absent/)).toBeVisible();

  // A gate is not a screen with disabled controls: there is nothing to submit.
  await expect(page.locator("main input, main textarea, main select, main button")).toHaveCount(0);
  await expect(page.locator(".problem")).toHaveCount(0);
});
