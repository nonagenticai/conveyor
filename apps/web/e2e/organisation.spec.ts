import { expect, test } from "./fixture";

// SPEC_V2 §10.5 — the live Organisation destination. The catalogue, its placements, its
// bindings and its departments read from the real actions; nothing here authors a release.

test("the organisation destination reads the catalogue rows", async ({ page }) => {
  await page.goto("/organisation");
  await expect(page.getByRole("heading", { name: "Organisation", exact: true })).toBeVisible();

  await expect(page.locator(".problem")).toHaveCount(0);
  await expect(page.locator(".tile")).toHaveCount(4);

  // Industries, Jobs, Bindings and Departments each render a real table, with rows or an
  // honest empty state.
  await expect(page.locator("table.table")).toHaveCount(4);
  for (const heading of ["Industries", "Jobs", "Bindings", "Departments"]) {
    await expect(page.getByRole("heading", { name: heading, exact: true })).toBeVisible();
  }
});

test("the organisation screen has no horizontal page scroll at 1280 and 390", async ({ page }) => {
  await page.goto("/organisation");
  await expect(page.getByRole("heading", { name: "Organisation", exact: true })).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `width ${width}`).toBeLessThanOrEqual(0);
  }
});
