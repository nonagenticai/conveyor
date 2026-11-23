import { expect, test } from "./fixture";

// T18-1 P1/P2: the seven nav groups, the V2 routes and the honest gates.

test("the shell shows the seven nav groups and the permitted destinations", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Today" })).toBeVisible();
  const groups = page.locator(".rail .grp");
  await expect(groups).toHaveText([
    "Portfolio & plan",
    "Organisation",
    "Procedures & authoring",
    "Govern & design",
    "Knowledge & memory",
    "Measure & improve",
    "Operations",
  ]);
  // The destinations whose reads the tenant can make are links.
  await expect(page.getByRole("link", { name: "Portfolio" })).toBeVisible();
  await expect(page.getByRole("link", { name: "Programme" })).toBeVisible();
  // A gated destination (super-admin only) is not announced to this tenant admin.
  await expect(page.getByRole("link", { name: "Platform" })).toHaveCount(0);
});

test("a signed-out V2 route gets the sign-in gate", async ({ browser, baseURL }) => {
  const context = await browser.newContext({
    baseURL,
    storageState: { cookies: [], origins: [] },
  });
  const page = await context.newPage();
  await page.goto("/portfolio");
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
  await context.close();
});

test("an unwired V2 destination names its owning slice", async ({ page }) => {
  await page.goto("/organisation");
  await expect(page.getByRole("heading", { name: "Organisation" })).toBeVisible();
  await expect(page.getByText("Not yet live")).toBeVisible();
  await expect(page.getByText(/owner: T0/)).toBeVisible();
});
