import { readFileSync } from "node:fs";

import { expect, test } from "@playwright/test";

// The live full matrix (S4, W7): after the real sign-in on the deployed game host, every
// top-level census route opens in-SPA with an h1 and no server error. The route list is read
// from the census, so it cannot drift from the instrument. Detail routes (`:id`) need a record
// id and stay proven in dev mode, as S1 did.
//
// Read-only: no write control is pressed. Run only through `npm run test:deployed`.

const census = JSON.parse(
  readFileSync(new URL("../../../../docs/evidence/walk-census.json", import.meta.url), "utf8"),
) as { routes: Array<{ walk: string }> };

const ROUTES = census.routes
  .map((row) => row.walk)
  .filter((walk) => !walk.includes(":"))
  .sort();

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

async function openRoute(page: any, route: string) {
  await page.getByRole("button", { name: "Palette" }).click();
  const dialog = page.getByRole("dialog", { name: "Palette" });
  await expect(dialog).toBeVisible();
  const search = dialog.getByRole("searchbox", { name: "Search" });
  await search.fill(route);
  await dialog
    .getByRole("button", { name: new RegExp(`^${escapeRegExp(route)}\\s`) })
    .first()
    .click();
  await expect(dialog).toHaveCount(0);
}

test("the deployed host signs in", async ({ page }) => {
  await page.goto("/");
  const gate = page.getByRole("button", { name: "Continue with Keycloak" });
  const today = page.getByRole("heading", { name: "Today" });
  await expect(gate.or(today)).toBeVisible({ timeout: 30_000 });
  if (await gate.isVisible()) await gate.click();
  await expect(today).toBeVisible({ timeout: 30_000 });
});

for (const route of ROUTES) {
  test(`${route} renders in the deployed walk`, async ({ page }) => {
    const serverErrors: string[] = [];
    page.on("response", (response: any) => {
      if (response.status() >= 500) {
        serverErrors.push(`${response.status()} ${new URL(response.url()).pathname}`);
      }
    });

    await page.goto("/");
    const gate = page.getByRole("button", { name: "Continue with Keycloak" });
    const today = page.getByRole("heading", { name: "Today" });
    await expect(gate.or(today)).toBeVisible({ timeout: 30_000 });
    if (await gate.isVisible()) await gate.click();
    await expect(today).toBeVisible({ timeout: 30_000 });

    await openRoute(page, route);
    await expect(page.locator("#screen h1").first()).toBeVisible({ timeout: 20_000 });
    await expect(page.locator(".problem")).toHaveCount(0);
    expect(serverErrors, `5xx responses: ${serverErrors.join(", ")}`).toEqual([]);
  });
}
