import { expect, test } from "@playwright/test";

// The world entrance: the canvas mounts (engine or the labelled fallback), the room sheet is
// the DOM path, and an absent sibling is an honest gate, never a fake screen.

test("the world mounts and the room map is the keyboard path", async ({ page }) => {
  await page.goto("/");
  const canvas = page.locator("#world-canvas");
  await expect(canvas).toBeVisible();
  // Strict: the suite builds the wasm, so a silent fallback is a defect, not an environment
  // (#839 — the config omitted the required `seed` and `ecs|fallback` hid it everywhere).
  await expect(canvas).toHaveAttribute("data-engine", "ecs", { timeout: 20_000 });

  const map = page.getByRole("navigation", { name: "Rooms" });
  await map.getByRole("button", { name: "Portfolio & plan" }).click();
  const dialog = page.getByRole("dialog", { name: "Portfolio & plan" });
  await expect(dialog).toBeVisible();
  // G0's entrance is live; an unbuilt sibling names its owner instead of opening.
  await expect(dialog.getByRole("button", { name: "Today" })).toBeEnabled();
  await expect(dialog.getByRole("button", { name: /not yet live/ }).first()).toBeDisabled();
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
});
