import { expect, test, type Page } from "@playwright/test";

// Runs FIRST (the `00-` prefix): the assertions below pin dev-a's seeded ledger to exactly 12
// calls, and later specs legitimately add `inference_call` rows when they execute a run (the
// cockpit's New run reaches the provider through the worker). The copilot spec follows the same
// rule by writing to a platform tenant instead of dev-a. A file whose fixtures are an exact
// baseline must observe it before anything else writes.

const baseURL = process.env.BASE_URL ?? "http://127.0.0.1:8931";

/** Swap the dev cookie. The file's storageState is alice; bob is the empty tenant. */
async function as(page: Page, login: string) {
  const host = new URL(baseURL).hostname;
  await page.context().addCookies([
    { name: "conveyor_dev", value: login, domain: host, path: "/" },
  ]);
}

/** A stat tile by its exact label: the detail line may also contain the word (e.g. "unpriced calls"). */
function tile(page: Page, label: string) {
  return page
    .locator(".tile")
    .filter({ has: page.locator(".tk", { hasText: new RegExp(`^${label}$`) }) });
}

test("usage reads the seeded ledger and filters the call table", async ({ page }) => {
  await page.goto("/measure/usage");
  await expect(page.getByRole("heading", { name: "Usage & cache" })).toBeVisible();

  // The dev ledger is 12 calls: 5 hits, 3 writes, 3 misses, 1 unknown.
  // The design's ratio is hits / (hits + misses) — 5 / 8 = 62.5%, shown as 63%.
  await expect(tile(page, "Cache hit ratio").locator(".tv")).toHaveText("63%");
  await expect(tile(page, "Calls").locator(".tv")).toHaveText("12");
  await expect(tile(page, "Writes never read").locator(".tv")).toHaveText("3");

  // The model table names the non-token unit column; the seed is token-only, so the cells are "—".
  await expect(page.locator("table.wtbl thead").first().getByText("Units")).toBeVisible();

  // Three day buckets, one 12-row call table, and the cache-hostile prompt sorts first.
  await expect(page.locator(".chart .col")).toHaveCount(3);
  await expect(page.locator("table.calls tbody tr")).toHaveCount(12);
  await expect(page.locator(".pill", { hasText: "never read" }).first()).toBeVisible();

  // A real filter through inference_calls.list: only write-class rows survive.
  await page.getByLabel("Cache class filter").selectOption("write");
  await expect.poll(async () => page.locator("table.calls tbody tr").count()).toBe(3);
  await expect
    .poll(async () => {
      const rows = await page.locator("table.calls tbody tr").allInnerTexts();
      return rows.every((row) => row.toLowerCase().includes("write"));
    })
    .toBe(true);

  await expect(page.locator(".problem")).toHaveCount(0);
});

test("an empty tenant says so instead of drawing a chart", async ({ page }) => {
  await as(page, "bob"); // dev-b: invited to exist, never given a ledger
  await page.goto("/measure/usage");
  await expect(page.getByText("No inference calls in this window")).toBeVisible();
  await expect(page.locator(".chart")).toHaveCount(0);
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("the screen has no horizontal page scroll at 1280 and 390", async ({ page }) => {
  await page.goto("/measure/usage");
  await expect(page.getByRole("heading", { name: "Usage & cache" })).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `width ${width}`).toBeLessThanOrEqual(0);
  }
});
