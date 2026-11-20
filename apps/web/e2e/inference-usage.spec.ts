import { expect, test, type Page } from "@playwright/test";

import { invoke } from "./helpers";

// Screen 11 — inference usage & cache. `00-usage.spec.ts` runs first and pins the seed's exact
// 12-call baseline before any other spec writes; this is the screen's own spec, so its happy
// path reads the same `inference_usage.report` the screen reads and never assumes a fixed total
// that later specs move.

type UsageReport = {
  rows: { bucket: string; calls: number; hits: number; misses: number; writes: number }[];
};

/** The 7-day window the screen defaults to, as the action takes it. */
function weekWindow(group_by: string) {
  const until = new Date();
  const since = new Date(until.getTime() - 7 * 86_400_000);
  return { since: since.toISOString(), until: until.toISOString(), group_by };
}

function tile(page: Page, label: string) {
  return page
    .locator(".tile")
    .filter({ has: page.locator(".tk", { hasText: new RegExp(`^${label}$`) }) });
}

test("the screen renders the report's own calls and ratio, and its filters are real", async ({
  page,
  request,
}) => {
  const report = await invoke<UsageReport>(request, "inference_usage.report", weekWindow("day"));
  const calls = report.rows.reduce((sum, row) => sum + row.calls, 0);
  const hits = report.rows.reduce((sum, row) => sum + row.hits, 0);
  const misses = report.rows.reduce((sum, row) => sum + row.misses, 0);
  const ratio = Math.round((hits / (hits + misses)) * 100);

  await page.goto("/measure/usage");
  await expect(page.getByRole("heading", { name: "Usage & cache" })).toBeVisible();
  await expect(tile(page, "Calls").locator(".tv")).toHaveText(String(calls));
  await expect(tile(page, "Cache hit ratio").locator(".tv")).toHaveText(`${ratio}%`);
  await expect(tile(page, "Writes never read")).toBeVisible();

  // The model dimension names the non-token unit column; the ledger's call table renders.
  await expect(page.locator("table.wtbl thead").first().getByText("Units")).toBeVisible();
  await expect(page.locator("table.calls tbody tr").first()).toBeVisible();

  // A real filter through `inference_calls.list`: only write-class rows survive.
  await page.getByLabel("Cache class filter").selectOption("write");
  await expect
    .poll(async () => {
      const rows = await page.locator("table.calls tbody tr").allInnerTexts();
      return rows.length > 0 && rows.every((row) => row.toLowerCase().includes("write"));
    })
    .toBe(true);
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("an empty tenant says so instead of drawing a chart", async ({ page }) => {
  // dev-b is invited to exist and never given a ledger.
  const base = process.env.BASE_URL ?? "http://127.0.0.1:8931";
  await page.context().addCookies([
    { name: "conveyor_dev", value: "bob", domain: new URL(base).hostname, path: "/" },
  ]);
  await page.goto("/measure/usage");
  await expect(page.getByText("No inference calls in this window")).toBeVisible();
  await expect(page.locator(".chart")).toHaveCount(0);
  await expect(page.locator(".problem")).toHaveCount(0);
});

// The route takes no id and every filter value it offers is valid, so the screen cannot be
// driven into a failed read from the UI. The error contract is the data source's: a window the
// server refuses is a refusal the screen would render as the shared ErrorPanel, never zeros.
test("the screen's source refuses an impossible window instead of inventing rows", async ({
  request,
}) => {
  const until = new Date();
  const since = new Date(until.getTime() + 60_000);
  await expect(
    invoke(request, "inference_usage.report", {
      since: since.toISOString(),
      until: until.toISOString(),
      group_by: "day",
    }),
  ).rejects.toThrow(/until/);
});
