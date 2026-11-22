import { expect, test } from "./fixture";

// SPEC_V2 §10.5 — the live Portfolio destination. The screen's only read is
// `portfolio.in_flight` against the seeded dev tenant: the in-flight set is computed from
// project status, so what the screen shows is what the action returned, never an invented row.

test("the portfolio destination renders the in-flight set from the real read", async ({ page }) => {
  await page.goto("/portfolio");
  await expect(page.getByRole("heading", { name: "Portfolio", exact: true })).toBeVisible();

  // The seeded admin's manifest carries portfolio.in_flight, so the read succeeds: a problem
  // panel here would be a broken contract, not an empty tenant.
  await expect(page.locator(".problem")).toHaveCount(0);

  // Four tiles count the two sets and the with/without-programme split, whatever the tenant's
  // set is.
  await expect(page.locator(".tile")).toHaveCount(4);

  // Either the tables carry rows, or the honest empty state is shown — never a fake row.
  const rows = page.locator("table tbody tr");
  const empty = page.getByText("Nothing is in flight");
  const count = await rows.count();
  const emptyVisible = await empty.isVisible().catch(() => false);
  expect(count > 0 || emptyVisible, "portfolio shows rows or the empty state").toBeTruthy();
});

test("the portfolio screen has no horizontal page scroll at 1280 and 390", async ({ page }) => {
  await page.goto("/portfolio");
  await expect(page.getByRole("heading", { name: "Portfolio", exact: true })).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `width ${width}`).toBeLessThanOrEqual(0);
  }
});
