import { expect, test } from "./fixture";

// SPEC_V2 §10.5 — the live Operations destination. The queue, hosts and providers are the
// tenant's own rows; the cluster reads either return their shape or refuse in their own words.
// Incidents are recorded by the governed `ops.incident` action, not on this screen.

test("the operations destination reads the estate's own rows", async ({ page }) => {
  await page.goto("/operations");
  await expect(page.getByRole("heading", { name: "Operations", exact: true })).toBeVisible();

  await expect(page.locator(".problem")).toHaveCount(0);
  await expect(page.locator(".tile")).toHaveCount(4);

  for (const heading of ["Queue", "Hosts", "Providers", "Cluster"]) {
    await expect(page.getByRole("heading", { name: heading, exact: true })).toBeVisible();
  }
});

test("the operations screen has no horizontal page scroll at 1280 and 390", async ({ page }) => {
  await page.goto("/operations");
  await expect(page.getByRole("heading", { name: "Operations", exact: true })).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `width ${width}`).toBeLessThanOrEqual(0);
  }
});
