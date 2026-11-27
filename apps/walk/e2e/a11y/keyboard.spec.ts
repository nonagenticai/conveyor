import { expect, test } from "@playwright/test";

// Keyboard-only: the skip link, a room sheet, Escape, and focus returned to the opener.

test("the skip link and the room sheet are a keyboard path with focus return", async ({
  page,
}) => {
  await page.goto("/");
  // The shell renders the map before the world finishes mounting, and `renderSession()` can
  // rebuild the map; `data-engine` is the settle point (either branch). Keyboarding into a
  // node that is replaced between focus and keypress is the flake this waits out.
  await expect(page.locator("#world-canvas")).toHaveAttribute(
    "data-engine",
    /ecs|fallback/,
    { timeout: 20_000 },
  );
  await page.getByRole("link", { name: "Skip to content" }).focus();
  await page.keyboard.press("Enter");
  await expect(page.locator("#screen")).toBeFocused();

  const map = page.getByRole("navigation", { name: "Rooms" });
  const room = map.getByRole("button", { name: "Portfolio & plan" });
  // `press` resolves and focuses the live node at action time, so a re-render cannot leave the
  // keypress addressed to a detached button.
  await room.press("Enter");
  const dialog = page.getByRole("dialog", { name: "Portfolio & plan" });
  await expect(dialog).toBeVisible();
  expect(await dialog.evaluate((node) => node.contains(document.activeElement))).toBe(true);

  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  expect(await room.evaluate((node) => node === document.activeElement)).toBe(true);
});
