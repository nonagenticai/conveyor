import { expect, test } from "@playwright/test";

// S12 §6 step 1. Two real modes, no mocked API: the default context carries a credential (the
// dev cookie locally, a Keycloak SSO session against a deployed app), the second deliberately
// has none.

const oidc = !process.env.DEV_AUTH_TOKEN;

test("a credential lands on Today", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Today" })).toBeVisible();
  await expect(page.getByText("Waiting on you")).toBeVisible();
  await expect(page.getByText("Runs that stopped")).toBeVisible();
  // /api/me answered: the shell knows who is signed in.
  await expect(page.locator(".who")).not.toHaveText("");
  // A load failure is its own panel; the happy path must not show it.
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("a browser without a credential gets the sign-in gate", async ({ browser, baseURL }) => {
  const context = await browser.newContext({ baseURL, storageState: { cookies: [], origins: [] } });
  const page = await context.newPage();
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
  if (oidc) {
    // A deployed config never offers the development form.
    await expect(page.getByRole("button", { name: "Continue with Keycloak" })).toBeVisible();
    await expect(page.getByLabel("Development login")).toHaveCount(0);
  } else {
    await expect(page.getByLabel("Development login")).toBeVisible();
    // The form is the real path a person takes: the server writes the dev cookie and the page
    // reloads into an authenticated boot (proved here, not assumed from the stored cookie).
    await page.getByLabel("Development login").fill("alice");
    await page.getByRole("button", { name: "Sign in (dev)" }).click();
    await expect(page.getByRole("heading", { name: "Today" })).toBeVisible({ timeout: 10_000 });
  }
  await context.close();
});
