import { expect, test, type Page } from "@playwright/test";

const baseURL = process.env.BASE_URL ?? "http://127.0.0.1:8931";

/**
 * Run as the platform admin in a platform tenant (kata/execution), not dev-a:
 * a copilot turn writes an `inference_call` ledger row, and dev-a's exact ledger is a
 * baseline assertion in usage.spec.ts. A third tenant keeps the seeded counts intact.
 */
async function asPlatformAdmin(page: Page) {
  const host = new URL(baseURL).hostname;
  await page.context().addCookies([
    { name: "conveyor_dev", value: "fr-test", domain: host, path: "/" },
  ]);
}

// Requires the API to be started with CONVEYOR_EXTERNAL_INFERENCE_URL pointing at the test
// provider: `node e2e/model-stub.mjs` on 8940 (see the file's header). The stub is an external
// provider double; the conversation, turn rows, run and SSE event under test are all real.

test("a conversation is created and the copilot answers through the provider", async ({ page }) => {
  await asPlatformAdmin(page);
  await page.goto("/copilot");
  await expect(page.getByRole("heading", { name: "Copilot" })).toBeVisible();
  await expect(page.getByText("No conversation open")).toBeVisible();

  await page.getByRole("button", { name: "Start a conversation" }).first().click();
  const composer = page.getByPlaceholder("Ask the copilot to do something in this tenant.");
  await expect(composer).toBeVisible();

  await composer.fill("hello copilot");
  await page.getByRole("button", { name: "Send" }).click();

  // The user turn and the assistant answer are two real rows, read back by GET …/turns.
  await expect(page.getByText("hello copilot")).toBeVisible();
  await expect(page.getByText("Stub answer: the copilot path works.")).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("the surface lists what the copilot may propose", async ({ page }) => {
  await asPlatformAdmin(page);
  await page.goto("/copilot");
  await expect(page.getByText("Surface")).toBeVisible();
  await expect(page.locator(".chips .chip").first()).toBeVisible();
});

test("the copilot screen has no horizontal page scroll at 1280 and 390", async ({ page }) => {
  await asPlatformAdmin(page);
  await page.goto("/copilot");
  await expect(page.getByRole("heading", { name: "Copilot" })).toBeVisible();
  // Measure after the surface has loaded, not while the first fetch is still settling.
  await expect(page.getByText("Surface")).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `width ${width}`).toBeLessThanOrEqual(0);
  }
});
