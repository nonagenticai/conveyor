import { defineConfig } from "@playwright/test";

const baseURL = process.env.BASE_URL ?? "http://127.0.0.1:8932";

export default defineConfig({
  testDir: "./e2e",
  timeout: 360_000,
  expect: { timeout: 20_000 },
  fullyParallel: false,
  workers: 1,
  reporter: [["list"], ["json", { outputFile: "test-results/spike-report.json" }]],
  use: {
    baseURL,
    channel: process.env.PLAYWRIGHT_CHANNEL ?? "chromium",
    trace: "retain-on-failure",
    storageState: {
      cookies: [
        {
          name: "conveyor_dev",
          value: "alice",
          domain: "127.0.0.1",
          path: "/",
          expires: -1,
          httpOnly: false,
          secure: false,
          sameSite: "Lax" as const,
        },
      ],
      origins: [],
    },
  },
});
