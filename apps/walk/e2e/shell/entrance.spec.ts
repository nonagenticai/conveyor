import { expect, test } from "@playwright/test";

// The foundation's first gate: the dev cookie signs in, the entrance is Today, the world and
// the room map are present. Nothing here needs the wasm engine.

test("the entrance signs in with the dev cookie and lands on Today", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Today" })).toBeVisible();
  await expect(page.getByRole("navigation", { name: "Rooms" })).toBeVisible();
  await expect(page.locator("#world-canvas")).toBeVisible();
  await expect(page.locator(".who")).not.toHaveText("");
  expect(errors).toEqual([]);
});

test("the palette is the route-and-action path", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Palette" }).click();
  const dialog = page.getByRole("dialog", { name: "Palette" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: /\/portfolio/ }).first().click();
  await expect(page.getByRole("heading", { name: "Portfolio" })).toBeVisible();
});
