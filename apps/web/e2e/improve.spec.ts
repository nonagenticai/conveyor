import { expect, test } from "./fixture";

// SPEC_V2 §10.5 — the live Improve destination. `metrics.flow` is read at call grain; a figure
// without a source ledger is listed unavailable with its reason, never rendered as zero. The
// kata queue reads `kata.list` (T5); adoption and revert stay governed actions.

test("the improve destination reads the flow figures and says what is unavailable", async ({
  page,
}) => {
  await page.goto("/improve");
  await expect(page.getByRole("heading", { name: "Improve", exact: true })).toBeVisible();

  await expect(page.locator(".problem")).toHaveCount(0);
  await expect(page.locator(".tile")).toHaveCount(4);

  // The unavailable list is a real panel either way: a table when a ledger is missing, the
  // honest empty sentence when every figure has one.
  await expect(page.getByRole("heading", { name: "Unavailable figures", exact: true })).toBeVisible();

  // The kata queue is a real read (kata.list): rows when the tenant has a kata, an honest
  // empty state otherwise. The locators are scoped to that panel — a page-wide row count could
  // be satisfied by the unavailable table above it — and the planned placeholder must be gone.
  const kata = page.locator("section.panel", { has: page.getByRole("heading", { name: "Kata queue" }) });
  await expect(kata).toBeVisible();
  await expect(kata.getByText("The kata queue is unavailable")).toHaveCount(0);
  const kataRows = kata.locator("table tbody tr");
  const kataEmpty = kata.getByText("No kata is recorded in this tenant");
  const rows = await kataRows.count();
  const emptyVisible = await kataEmpty.isVisible().catch(() => false);
  expect(rows > 0 || emptyVisible, "the kata queue shows rows or the empty state").toBeTruthy();
});

test("the improve screen has no horizontal page scroll at 1280 and 390", async ({ page }) => {
  await page.goto("/improve");
  await expect(page.getByRole("heading", { name: "Improve", exact: true })).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `width ${width}`).toBeLessThanOrEqual(0);
  }
});
