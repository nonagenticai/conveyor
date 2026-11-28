import { defineConfig } from "@playwright/test";

// Two modes, one runner (the apps/web convention):
//   BASE_URL set      -> target that origin, start no web server (the deployed game host)
//   DEV_AUTH_TOKEN set -> the dev cookie (local/CI); unset -> real OIDC with E2E_USER/E2E_PASS
const baseURL = process.env.BASE_URL ?? "http://127.0.0.1:8932";
const base = new URL(baseURL);
const token = process.env.DEV_AUTH_TOKEN;
const devLogin = token?.startsWith("dev:") ? token.slice(4) : token;
const oidc = Boolean(process.env.BASE_URL) && !process.env.DEV_AUTH_TOKEN;
const OIDC_STATE = "test-results/oidc-state.json";

export default defineConfig({
  testDir: "./e2e",
  testIgnore: oidc ? undefined : /deployed\//,
  timeout: 60_000,
  expect: { timeout: 10_000 },
  fullyParallel: false,
  workers: 1,
  reporter: [
    ["list"],
    ["json", { outputFile: process.env.WALK_REPORT ?? "test-results/walk-report.json" }],
  ],
  use: {
    baseURL,
    channel: process.env.PLAYWRIGHT_CHANNEL ?? "chrome",
    // #916. The performance budgets read `performance.memory.usedJSHeapSize`, and Chrome floors
    // that value at 10,000,000 bytes unless this switch is set: a real heap of 2.6 MB is reported
    // as exactly 10000000. A ratio taken without it compares a floor to a floor, so the soak
    // either passes trivially or reports growth that is mostly the floor giving way — which is
    // how a 1.89x was recorded against a client whose GC'd heap is flat. Measured 2026-11-28:
    // playwright-core 1.65.0 sets no such switch, and the same page read 10000000 without it and
    // 2623436 with it.
    launchOptions: { args: ["--enable-precise-memory-info"] },
    trace: "retain-on-failure",
    storageState: devLogin
      ? {
          cookies: [
            {
              name: "conveyor_dev",
              value: devLogin,
              domain: base.hostname,
              path: "/",
              expires: -1,
              httpOnly: false,
              secure: base.protocol === "https:",
              sameSite: "Lax" as const,
            },
          ],
          origins: [],
        }
      : undefined,
  },
  projects: oidc
    ? [
        { name: "oidc", testMatch: /deployed\/oidc\.setup\.ts/ },
        {
          // OIDC mode targets the live host. Only the read-only `deployed/` specs may run there:
          // the wing specs write (against the dev seed), and an unscoped deployed run would
          // perform those writes in production.
          name: "app",
          testMatch: /deployed\/.*\.spec\.ts$/,
          dependencies: ["oidc"],
          use: { storageState: OIDC_STATE },
        },
      ]
    : undefined,
  webServer: process.env.BASE_URL
    ? undefined
    : {
        command: "node dev-server.mjs",
        url: `${baseURL}/__health`,
        reuseExistingServer: true,
        timeout: 30_000,
      },
});
