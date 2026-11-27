import { expect, test } from "@playwright/test";

// Keyboard-only: the skip link, a room sheet, Escape, and focus returned to the opener.

test("the skip link and the room sheet are a keyboard path with focus return", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("link", { name: "Skip to content" }).focus();
  await page.keyboard.press("Enter");
  await expect(page.locator("#screen")).toBeFocused();

  const map = page.getByRole("navigation", { name: "Rooms" });
  const room = map.getByRole("button", { name: "Portfolio & plan" });
  await room.focus();
  await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog", { name: "Portfolio & plan" });
  await expect(dialog).toBeVisible();
  expect(await dialog.evaluate((node) => node.contains(document.activeElement))).toBe(true);

  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  expect(await room.evaluate((node) => node === document.activeElement)).toBe(true);
});
