import { expect, test } from "@playwright/test";

// The live proof for S1: after the real `conveyor-e2e` sign-in on the deployed game host, S1's
// twelve top-level routes render in-SPA with an h1 and no server error. The three detail routes
// (/runs/:id, /meetings/:id, /board/:board_id) need a record id and are proven in dev mode.
// Read-only: no write control is pressed. Run only through `npm run test:deployed` (D1 scopes
// OIDC mode to this directory).

const ROUTES = [
  "/organisation",
  "/project-types",
  "/agents",
  "/jobs",
  "/tools",
  "/programme",
  "/decisions",
  "/validation",
  "/execution",
  "/runs",
  "/meetings",
  "/plan",
];

function literal(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

test("S1's routes render on the deployed game host", async ({ page }) => {
  const serverErrors: string[] = [];
  page.on("response", (response) => {
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

  for (const route of ROUTES) {
    await page.getByRole("button", { name: "Palette" }).click();
    const palette = page.getByRole("dialog", { name: "Palette" });
    await palette.getByRole("searchbox", { name: "Search" }).fill(route);
    await palette
      .getByRole("button", { name: new RegExp(`^${literal(route)}`) })
      .first()
      .click();
    const heading = page.locator("#screen h1");
    await expect(heading, route).toBeVisible({ timeout: 30_000 });
    await expect(heading, route).not.toHaveText("");
  }

  expect(serverErrors, `5xx responses: ${serverErrors.join(", ")}`).toEqual([]);
});
