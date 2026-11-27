import { expect, test } from "@playwright/test";

// A signed-out deep link is the gate, and a dev sign-in returns to the path that was opened.

test.use({ storageState: { cookies: [], origins: [] } });

test("a signed-out deep link is the gate and returns after dev login", async ({ page }) => {
  await page.goto("/portfolio");
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
  await page.getByRole("button", { name: "Continue as dev user" }).click();
  await expect(page).toHaveURL(/\/portfolio$/);
  await expect(page.getByRole("heading", { name: "Portfolio" })).toBeVisible();
});

test("sign-out returns to the gate", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Sign out" }).click();
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
});
