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
  // `press` resolves and focuses the live node at action time, like the room button below: the
  // shell rebuilds the skip link and the map while the world settles, and a `focus()` followed by
  // a global key press can address a node that has already been replaced (measured once in a
  // 224-test run; the assertion is unchanged).
  await page.getByRole("link", { name: "Skip to content" }).press("Enter");
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

// #914. The Palette button's tooltip advertises Ctrl/Cmd+K; this proves the advertised path exists
// and that the shortcut never steals a keystroke from a field.
test("the advertised Ctrl/Cmd+K opens the Palette, and a field keeps its keystrokes", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.locator("#world-canvas")).toHaveAttribute(
    "data-engine",
    /ecs|fallback/,
    { timeout: 20_000 },
  );

  // The shortcut is deliberately ignored while a field has focus, and `focusFirst` may have put
  // focus on a control inside the entrance. Blur first so this asserts the body-level path.
  await page.evaluate(() => {
    const active = document.activeElement;
    if (active instanceof HTMLElement) active.blur();
  });
  const shortcut = process.platform === "darwin" ? "Meta+k" : "Control+k";
  await page.keyboard.press(shortcut);
  const dialog = page.getByRole("dialog", { name: "Palette" });
  await expect(dialog).toBeVisible();

  const search = dialog.getByRole("searchbox", { name: "Search" });
  await expect(search).toBeFocused();
  await page.keyboard.press("k");
  await expect(search).toHaveValue("k");
  // The shortcut is ignored while a field has focus, so the second press cannot stack a palette.
  await page.keyboard.press(shortcut);
  await expect(page.getByRole("dialog", { name: "Palette" })).toHaveCount(1);

  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
});
