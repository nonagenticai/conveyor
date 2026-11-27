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
  id?: string,
): Promise<Usage> {
  return invoke<Usage>(request, "metrics.usage", {
    level,
    ...(id ? { id } : {}),
    ...weekWindow(),
  });
}

function tile(page: Page, label: string) {
  return page
    .locator(".tile")
    .filter({ has: page.locator(".tk", { hasText: new RegExp(`^${label}$`) }) });
}

/**
 * The screen's table against the action's own rows, sampled together.
 *
 * The ledger is live while this suite runs — `web.yml` starts a worker and the model double, and
 * both land inference calls — so a row count read before the navigation can be a row stale by
 * the time the table renders (measured 2026-11-27: 151 read, 152 rendered). Re-read both sides
 * until they agree; the assertion is still that the screen is the action's own page, never a
 * client-side approximation.
 */
async function expectRowsToMatch(
  page: Page,
  request: APIRequestContext,
  level: string,
  id?: string,
): Promise<void> {
  await expect
    .poll(
      async () =>
        (await usage(request, level, id)).rows.length -
        (await page.getByTestId("metering-table").locator("tbody tr").count()),
      { timeout: 10_000 },
    )
    .toBe(0);
}

test("the roll-ups drill to the call", async ({ page, request }) => {
  await page.goto("/metering");
  await expect(page.getByRole("heading", { name: "Metering" })).toBeVisible();

  // The roll-up's tiles are the sums of the rows the action returns — sampled in the same poll
  // iteration, because the ledger is live while this suite runs (measured 2026-11-27: 150 read,
  // 151 shown, a call landing between the read and the render).
  const tileNumber = async (label: string): Promise<number> => {
    const text = await tile(page, label).locator(".tv").textContent();
    return Number((text ?? "").replace(/[^0-9]/g, ""));
  };
  await expect
    .poll(
      async () => {
        const rows = (await usage(request, "tenant")).rows;
        const calls = rows.reduce((total, row) => total + row.calls, 0);
        const tokens = rows.reduce((total, row) => total + row.tokens, 0);
        const shownCalls = await tileNumber("Calls");
        const shownTokens = await tileNumber("Tokens");
        return `${shownCalls - calls}/${shownTokens - tokens}`;
      },
      { timeout: 10_000 },
    )
    .toBe("0/0");

  // The tenant level is one named row; the table is the action's own page.
  await expectRowsToMatch(page, request, "tenant");

  // The drill-down levels render from the same action; the call level is one row per call.
  await page.getByRole("tab", { name: "Call", exact: true }).click();
  await expectRowsToMatch(page, request, "call");
  await expect(page.getByTestId("metering-table").locator("tbody tr").first()).toContainText("call");

  // A selected run scopes steps and calls by run_id — the one parent the action's filter names.
  const run = await usage(request, "run");
  const scoped = run.rows.find((row) => row.key.length === 36);
  await page.getByRole("tab", { name: "Run", exact: true }).click();
  await expectRowsToMatch(page, request, "run");
  if (scoped) {
    // The full key, not eight characters: a UUIDv7's timestamp prefix repeats within a
    // millisecond, so a prefix match is ambiguous when many runs were seeded together.
    await page
      .locator(`tr[data-run="${scoped.key}"]`)
      .getByRole("button", { name: "Drill to calls" })
      .click();
    await expect(page.getByTestId("metering-scope")).toBeVisible();
    // Same live-ledger sampling, scoped to the one run.
    await expectRowsToMatch(page, request, "call", scoped.key);
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
