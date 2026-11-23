import { expect, test } from "./fixture";

// SPEC_V2 §10.5/§5.7 — the live Authoring destination. `authoring.assets` is the catalogue
// read; intake, draft, review and publish are governed actions, never this screen's write. A
// chat message or document can never grant a tool, widen a ceiling or change an approval policy.

test("the authoring destination lists the asset catalogue", async ({ page }) => {
  await page.goto("/authoring");
  await expect(page.getByRole("heading", { name: "Authoring", exact: true })).toBeVisible();

  await expect(page.locator(".problem")).toHaveCount(0);
  await expect(page.getByLabel("Kind")).toBeVisible();
  await expect(page.getByLabel("Status")).toBeVisible();
  await expect(page.locator(".tile")).toHaveCount(4);
  await expect(page.locator("table.table")).toHaveCount(1);
  await expect(
    page.getByRole("heading", { name: "From intake to publication", exact: true }),
  ).toBeVisible();

  const rows = page.locator("table tbody tr");
  const empty = page.getByText("No asset in this tenant carries that kind and status.");
  const count = await rows.count();
  const emptyVisible = await empty.isVisible().catch(() => false);
  expect(count > 0 || emptyVisible, "authoring shows rows or the empty state").toBeTruthy();
});

test("the authoring screen has no horizontal page scroll at 1280 and 390", async ({ page }) => {
  await page.goto("/authoring");
  await expect(page.getByRole("heading", { name: "Authoring", exact: true })).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `width ${width}`).toBeLessThanOrEqual(0);
  }
});
