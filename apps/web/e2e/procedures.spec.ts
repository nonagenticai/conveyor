import { expect, test } from "./fixture";

// SPEC_V2 §10.5 — the live Procedures destination. One read (`procedures.list`), filtered by
// status; a procedure is the executable form of an SOP, never an authored script here.

test("the procedures destination lists the tenant's procedures", async ({ page }) => {
  await page.goto("/procedures");
  await expect(page.getByRole("heading", { name: "Procedures", exact: true })).toBeVisible();

  await expect(page.locator(".problem")).toHaveCount(0);
  await expect(page.getByLabel("Status")).toBeVisible();
  await expect(page.locator(".tile")).toHaveCount(4);

  // One real table: rows when the tenant has procedures, the honest empty state otherwise.
  await expect(page.locator("table.table")).toHaveCount(1);
  const rows = page.locator("table tbody tr");
  const empty = page.getByText("No procedure in this tenant carries that status.");
  const count = await rows.count();
  const emptyVisible = await empty.isVisible().catch(() => false);
  expect(count > 0 || emptyVisible, "procedures shows rows or the empty state").toBeTruthy();
});

test("the procedures screen has no horizontal page scroll at 1280 and 390", async ({ page }) => {
  await page.goto("/procedures");
  await expect(page.getByRole("heading", { name: "Procedures", exact: true })).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `width ${width}`).toBeLessThanOrEqual(0);
  }
});
