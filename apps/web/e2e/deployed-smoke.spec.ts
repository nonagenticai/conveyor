import { expect, test } from "@playwright/test";

// The deployed release smoke — S14 P3.2's REQUIRED spec, run against a deployed origin with the
// real realm (`BASE_URL=https://conveyor.nonagentic.ai`, OIDC mode, `conveyor-e2e`).
//
// Why the button: the access token is memory-only by design (ADR 0001). A new browser context
// carries the Keycloak SSO cookies, not the token, so the app boots to the sign-in gate; the
// gate's button then completes the auth-code round trip with no credential entry (the SSO
// session answers). Measured 2026-11-20: without this click, `shell-login`'s "credential lands
// on Today" assumption holds only inside the setup context, not in later ones.
//
// The rest of `e2e/` is the DEVELOPMENT suite: its specs create or expect seeded fixtures and
// are not the deployed required set.
test("the deployed console signs in through SSO and reads the programme", async ({ page }) => {
  const serverErrors: string[] = [];
  page.on("response", (response) => {
    if (response.status() >= 500) serverErrors.push(`${response.status()} ${response.url()}`);
  });

  await page.goto("/");
  const gate = page.getByRole("button", { name: "Continue with Keycloak" });
  const today = page.getByRole("heading", { name: "Today" });
  // The WASM shell renders after load: wait for either outcome before deciding.
  await expect(gate.or(today)).toBeVisible({ timeout: 30_000 });
  if (await gate.isVisible()) {
    await gate.click();
  }
  await expect(today).toBeVisible({ timeout: 30_000 });
  await expect(page.locator(".problem")).toHaveCount(0);
  await expect(page.locator(".who")).not.toHaveText("");

  // In-SPA navigation only: the access token is memory-only, so a HARD navigation re-boots the
  // WASM shell and lands back on the sign-in gate (measured 2026-11-20). Clicking the rail keeps
  // the authenticated session.
  for (const title of ["Board", "Runs", "Search"] as const) {
    await page.getByRole("link", { name: title }).first().click();
    await expect(page.locator("h1").first()).toBeVisible({ timeout: 15_000 });
    await expect(page.locator(".problem")).toHaveCount(0);
    await expect(page.locator("body")).toContainText(title);
  }

  expect(serverErrors, `5xx responses: ${serverErrors.join(", ")}`).toEqual([]);
});
