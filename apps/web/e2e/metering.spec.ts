import { expect, test, type APIRequestContext, type Page } from "./fixture";

import { invoke } from "./helpers";

// T18-4 (#565) P2 — Metering. Every number is the action's own rows over the screen's window;
// the test reads the same action and compares, so the screen cannot drift into client-side
// arithmetic. The drill to `call` is the level path the action serves; the run→step/call drill
// is offered only for a UUID run key (the action's filter is `run_id`).

type UsageRow = {
  key: string;
  calls: number;
  tool_calls: number;
  tokens: number;
  cost_usd: number;
  tool_cost_usd: number;
  runs: number;
};

type Usage = { level: string; rows: UsageRow[] };

/** The 7-day window the screen defaults to, as the action takes it. */
function weekWindow() {
  const until = new Date();
  const since = new Date(until.getTime() - 7 * 86_400_000);
  return { since: since.toISOString(), until: until.toISOString() };
}

async function usage(
  request: APIRequestContext,
  level: string,
): Promise<Usage> {
  return invoke<Usage>(request, "metrics.usage", { level, ...weekWindow() });
}

function tile(page: Page, label: string) {
  return page
    .locator(".tile")
    .filter({ has: page.locator(".tk", { hasText: new RegExp(`^${label}$`) }) });
}

test("the roll-ups drill to the call", async ({ page, request }) => {
  const tenant = await usage(request, "tenant");
  const calls = tenant.rows.reduce((sum, row) => sum + row.calls, 0);
  const tokens = tenant.rows.reduce((sum, row) => sum + row.tokens, 0);
  const callLevel = await usage(request, "call");

  await page.goto("/metering");
  await expect(page.getByRole("heading", { name: "Metering" })).toBeVisible();

  // The roll-up's tiles are the sums of the rows the action returned.
  await expect(tile(page, "Calls").locator(".tv")).toHaveText(calls.toLocaleString("en-US"));
  await expect(tile(page, "Tokens").locator(".tv")).toHaveText(tokens.toLocaleString("en-US"));

  // The tenant level is one named row; the table is the action's own page.
  await expect(page.getByTestId("metering-table").locator("tbody tr")).toHaveCount(tenant.rows.length);

  // The drill-down levels render from the same action; the call level is one row per call.
  await page.getByRole("tab", { name: "Call", exact: true }).click();
  await expect(page.getByTestId("metering-table").locator("tbody tr")).toHaveCount(
    callLevel.rows.length,
  );
  await expect(page.getByTestId("metering-table").locator("tbody tr").first()).toContainText("call");

  // A selected run scopes steps and calls by run_id — the one parent the action's filter names.
  const run = await usage(request, "run");
  const scoped = run.rows.find((row) => row.key.length === 36);
  await page.getByRole("tab", { name: "Run", exact: true }).click();
  await expect(page.getByTestId("metering-table").locator("tbody tr")).toHaveCount(run.rows.length);
  if (scoped) {
    // The full key, not eight characters: a UUIDv7's timestamp prefix repeats within a
    // millisecond, so a prefix match is ambiguous when many runs were seeded together.
    await page
      .locator(`tr[data-run="${scoped.key}"]`)
      .getByRole("button", { name: "Drill to calls" })
      .click();
    await expect(page.getByTestId("metering-scope")).toBeVisible();
    const runCalls = await invoke<Usage>(request, "metrics.usage", {
      level: "call",
      id: scoped.key,
      ...weekWindow(),
    });
    await expect(page.getByTestId("metering-table").locator("tbody tr")).toHaveCount(
      runCalls.rows.length,
    );
  } else {
    // The dev seed's ledger is unattributed: no run key can scope, so no drill control is
    // offered for it — the screen never invents a parent the action cannot filter by.
    await expect(
      page.getByTestId("metering-table").getByRole("button", { name: "Drill to calls" }),
    ).toHaveCount(0);
    await expect(page.getByTestId("metering-scope")).toHaveCount(0);
  }
});

test("the metering screen has no horizontal page scroll at 1280 and 390", async ({ page }) => {
  await page.goto("/metering");
  await expect(page.getByRole("heading", { name: "Metering" })).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `width ${width}`).toBeLessThanOrEqual(0);
  }
});
