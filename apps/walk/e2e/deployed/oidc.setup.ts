import { mkdirSync } from "node:fs";

import { expect, test } from "@playwright/test";

// The deployed login, mirroring apps/web/e2e/oidc.setup.ts: one real auth-code + PKCE round
// trip as `conveyor-e2e`, then the SSO cookies go into storageState so later contexts
// re-authenticate silently. Credentials are env-only: E2E_USER / E2E_PASS. Nothing committed.

const STATE_FILE = "test-results/oidc-state.json";

test("sign in through Keycloak once and keep the session", async ({ page, context }) => {
  const user = process.env.E2E_USER;
  const password = process.env.E2E_PASS;
  test.skip(
    !user || !password,
    "E2E_USER and E2E_PASS are required for the deployed (OIDC) suite",
  );

  await page.goto("/");
  await page.getByRole("button", { name: "Continue with Keycloak" }).click();
  await page.locator("#username").fill(user);
  await page.locator("#password").fill(password);
  await page.locator("#kc-login").click();

  await expect(page.getByRole("heading", { name: "Today" })).toBeVisible({ timeout: 30_000 });
  mkdirSync("test-results", { recursive: true });
  await context.storageState({ path: STATE_FILE });
});
