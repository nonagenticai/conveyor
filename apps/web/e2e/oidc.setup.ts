import { mkdirSync, writeFileSync } from "node:fs";

import { expect, test } from "@playwright/test";

// W10: the deployed-suite login. One real auth-code + PKCE round trip through the realm's
// `conveyor-e2e` user; the access token is captured from the app's own API request (the token
// is memory-only in the browser, never localStorage) and written for helpers.ts. The SSO
// cookies go into storageState so later contexts re-authenticate silently.
//
// Credentials are env-only: E2E_USER / E2E_PASS. Nothing is committed.

const TOKEN_FILE = process.env.E2E_TOKEN_FILE ?? "test-results/oidc-token.txt";
const STATE_FILE = "test-results/oidc-state.json";

test("sign in through Keycloak once and keep the session", async ({ page, context }) => {
  const user = process.env.E2E_USER;
  const password = process.env.E2E_PASS;
  test.skip(
    !user || !password,
    "E2E_USER and E2E_PASS are required for the deployed (OIDC) suite",
  );

  let token = "";
  page.on("request", (request) => {
    const header = request.headers()["authorization"];
    if (!token && header?.startsWith("Bearer ") && request.url().includes("/api/")) {
      token = header.slice("Bearer ".length);
    }
  });

  await page.goto("/");
  // The dev gate must not be offered in production; the OIDC button is the only way in.
  await page.getByRole("button", { name: "Continue with Keycloak" }).click();
  await page.locator("#username").fill(user);
  await page.locator("#password").fill(password);
  await page.locator("#kc-login").click();

  await expect(page.getByRole("link", { name: "Today" })).toBeVisible({ timeout: 30_000 });
  await expect
    .poll(() => token, { timeout: 30_000, intervals: [250, 500, 1000] })
    .not.toBe("");

  mkdirSync("test-results", { recursive: true });
  writeFileSync(TOKEN_FILE, token);
  await context.storageState({ path: STATE_FILE });
});
