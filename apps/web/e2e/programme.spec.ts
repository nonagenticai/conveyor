import { expect, test } from "./fixture";

// SPEC_V2 §10.5 — the live Programme destination. Four reads, all against the selected
// programme: tranches.list, cadences.list, benefits.list and lessons.list. The screen is
// read-only by design: adoption and approval are governed actions on their own destinations.

test("the programme destination reads the selected programme's rows", async ({ page }) => {
  await page.goto("/programme");
  await expect(page.getByRole("heading", { name: "Programme", exact: true })).toBeVisible();

  // The seeded admin's manifest carries the reads, so a problem panel would be a broken
  // contract rather than an empty tenant.
  await expect(page.locator(".problem")).toHaveCount(0);

  // The programme picker and the four count tiles are always present.
  await expect(page.getByLabel("Programme")).toBeVisible();
  await expect(page.locator(".tile")).toHaveCount(4);

  // Each of the four panels is a real table: rows when the programme has them, an honest
  // empty state when it does not. Never a fabricated row.
  await expect(page.locator("table.table")).toHaveCount(4);
  for (const heading of ["Tranches", "Cadences", "Benefits", "Lessons"]) {
    await expect(page.getByRole("heading", { name: heading, exact: true })).toBeVisible();
  }
});

test("the programme screen has no horizontal page scroll at 1280 and 390", async ({ page }) => {
  await page.goto("/programme");
  await expect(page.getByRole("heading", { name: "Programme", exact: true })).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `width ${width}`).toBeLessThanOrEqual(0);
  }
});
