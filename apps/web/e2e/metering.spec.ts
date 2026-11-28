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

/** The action's own row count at one level, read now. */
async function rowsNow(
  request: APIRequestContext,
  level: string,
  id?: string,
): Promise<number> {
  return (await usage(request, level, id)).rows.length;
}

/**
 * The table against the action's own rows, bracketed rather than equated.
 *
 * The ledger is live while this suite runs — `web.yml` starts a worker and the model double, and
 * both land inference calls — so a count taken at one moment and a table rendered from a read at
 * another can differ in either direction (measured 2026-11-27: 151 read / 152 rendered, then 150
 * rendered / 151 read). What is knowable is the bracket: the screen read the action *after*
 * `before` was taken and *before* this call, so the table must show a count the ledger held in
 * that window. The window also waits for the tab to render: until it does, the table carries the
 * previous level's count, which falls outside the bracket and the poll tries again.
 */
async function expectRowsWithin(
  page: Page,
  request: APIRequestContext,
  level: string,
  before: number,
  id?: string,
): Promise<void> {
  await expect
    .poll(
      async () => {
        const shown = await page.getByTestId("metering-table").locator("tbody tr").count();
        const now = await rowsNow(request, level, id);
        return shown >= before && shown <= now
          ? "within"
          : `${shown} rows, the ledger held ${before}..${now}`;
      },
      { timeout: 10_000 },
    )
    .toBe("within");
}

test("the roll-ups drill to the call", async ({ page, request }) => {
  const rollup = async () => {
    const rows = (await usage(request, "tenant")).rows;
    return {
      calls: rows.reduce((total, row) => total + row.calls, 0),
      tokens: rows.reduce((total, row) => total + row.tokens, 0),
      rows: rows.length,
    };
  };
  const before = await rollup();

  await page.goto("/metering");
  await expect(page.getByRole("heading", { name: "Metering" })).toBeVisible();

  // The roll-up's tiles are the sums of the rows the action returns. The screen reads once, so
  // the read is bracketed: `before` is the ledger at navigation, `after` the ledger as soon as
  // the tiles have rendered, and each tile must hold a value the ledger carried in between.
  const tileNumber = async (label: string): Promise<number> => {
    const text = await tile(page, label).locator(".tv").textContent();
    return Number((text ?? "").replace(/[^0-9]/g, ""));
  };
  const shownCalls = await tileNumber("Calls");
  const shownTokens = await tileNumber("Tokens");
  const shownRows = await page.getByTestId("metering-table").locator("tbody tr").count();
  const after = await rollup();

  // The window is the screen's own load, in which this suite lands at most a call or two. A
  // ledger that moved further means the bracket is not meaningful and the test says so.
  expect(after.calls - before.calls).toBeLessThanOrEqual(2);
  expect(
    shownCalls,
    `the Calls tile shows ${shownCalls}; the ledger held ${before.calls}..${after.calls}`,
  ).toBeGreaterThanOrEqual(before.calls);
  expect(shownCalls).toBeLessThanOrEqual(after.calls);
  expect(shownTokens).toBeGreaterThanOrEqual(before.tokens);
  expect(shownTokens).toBeLessThanOrEqual(after.tokens);
  expect(shownRows).toBeGreaterThanOrEqual(before.rows);
  expect(shownRows).toBeLessThanOrEqual(after.rows);

  // The drill-down levels render from the same action; the call level is one row per call.
  const callsBefore = await rowsNow(request, "call");
  await page.getByRole("tab", { name: "Call", exact: true }).click();
  await expectRowsWithin(page, request, "call", callsBefore);
  await expect(page.getByTestId("metering-table").locator("tbody tr").first()).toContainText("call");

  // A selected run scopes steps and calls by run_id — the one parent the action's filter names.
  const run = await usage(request, "run");
  const scoped = run.rows.find((row) => row.key.length === 36);
  const runsBefore = await rowsNow(request, "run");
  await page.getByRole("tab", { name: "Run", exact: true }).click();
  await expectRowsWithin(page, request, "run", runsBefore);
  if (scoped) {
    // The full key, not eight characters: a UUIDv7's timestamp prefix repeats within a
    // millisecond, so a prefix match is ambiguous when many runs were seeded together.
    const scopedBefore = await rowsNow(request, "call", scoped.key);
    await page
      .locator(`tr[data-run="${scoped.key}"]`)
      .getByRole("button", { name: "Drill to calls" })
      .click();
    await expect(page.getByTestId("metering-scope")).toBeVisible();
    await expectRowsWithin(page, request, "call", scopedBefore, scoped.key);
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
