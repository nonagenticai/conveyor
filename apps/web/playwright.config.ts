import { defineConfig } from "@playwright/test";

// S12 §6 step 0: one helper, two auth modes.
//   BASE_URL set      -> target that origin, start no web server (deployed app / S14 rehearsal)
//   DEV_AUTH_TOKEN set -> the dev cookie (local); unset -> real OIDC with E2E_USER/E2E_PASS
// The dev cookie is `conveyor_dev=<login>`; the CLI transport uses `Authorization: Bearer dev:<login>`.
const baseURL = process.env.BASE_URL ?? "http://127.0.0.1:8931";
const base = new URL(baseURL);
const token = process.env.DEV_AUTH_TOKEN;
const devLogin = token?.startsWith("dev:") ? token.slice(4) : token;
// S12 §6 step 0: a deployed target with no dev token is the real-OIDC mode. The setup project
// logs in once through Keycloak, writes the access token the API helpers use, and keeps the
// SSO cookies so every later context re-authenticates silently.
const oidc = Boolean(process.env.BASE_URL) && !process.env.DEV_AUTH_TOKEN;
const OIDC_STATE = "test-results/oidc-state.json";

export default defineConfig({
  testDir: "./e2e",
  testIgnore: oidc ? undefined : /oidc\.setup\.ts/,
  timeout: 60_000,
  expect: { timeout: 10_000 },
  fullyParallel: false,
  workers: 1,
  reporter: [["list"]],
  use: {
    baseURL,
    // The S01 box's bundled Chromium download was incomplete; Chrome is installed and is the
    // engine family the ADR measured. Playwright 1.65 also drives the bundled browser if set.
    channel: process.env.PLAYWRIGHT_CHANNEL ?? "chrome",
    trace: "retain-on-failure",
    storageState: devLogin
      ? {
          // domain/path, not url: the `request` fixture's APIRequestContext rejects the
          // url form, and it is the same cookie for both browser and API calls.
          cookies: [
            {
              name: "conveyor_dev",
              value: devLogin,
              domain: base.hostname,
              path: "/",
              expires: -1,
              httpOnly: false,
              secure: false,
              sameSite: "Lax" as const,
            },
          ],
          origins: [],
        }
      : oidc
        ? OIDC_STATE
        : undefined,
  },
  projects: oidc
    ? [
        { name: "oidc", testMatch: /oidc\.setup\.ts/ },
        {
          name: "app",
          testIgnore: /oidc\.setup\.ts/,
          dependencies: ["oidc"],
          use: { storageState: OIDC_STATE },
        },
      ]
    : undefined,
  // With BASE_URL exported the caller owns the server (S12 gate). Otherwise this serves the
  // built site and proxies /api to the real binary on :8930.
  webServer: process.env.BASE_URL
    ? undefined
    : {
        command: "node dev-server.mjs",
        url: `${baseURL}/__health`,
        reuseExistingServer: true,
        timeout: 30_000,
      },
});
