import { expect, test } from "@playwright/test";

// Screen 14 — Improvement. The fixture is the seeded suite and its two control runs (gold and
// empty) in dev-a; nothing is route-mocked. Two facts this spec exists to hold:
//   * a point with no baseline is shown as a point, never plotted against one;
//   * a candidate that cannot start says why in the API's own words, never a fake run.

test("the seeded suite shows its control points, and no control is offered a baseline", async ({
  page,
}) => {
  await page.goto("/evals/improvement");
  await expect(page.getByRole("heading", { name: "Improvement", exact: true })).toBeVisible();

  // The two seeded controls, each a point with its own denominator.
  await expect(page.locator("table tbody tr")).toHaveCount(2);
  await expect(page.locator("table tbody")).toContainText("gold");
  await expect(page.locator("table tbody")).toContainText("empty");
  await expect(page.locator("table tbody")).toContainText("8/8");
  await expect(page.locator("table tbody")).toContainText("0/8");

  // No baseline is promoted, so no point is plotted against one — and nothing claims otherwise.
  await expect(page.locator("table tbody")).toContainText("no baseline");
  await expect(page.locator(".tile", { hasText: "Baseline" }).first()).toContainText("none");
  await expect(page.locator(".tile", { hasText: "Comparable" })).toContainText("0/2");

  // A control is never offered as the subject baseline.
  await expect(page.getByRole("button", { name: "Promote" })).toHaveCount(0);
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("a candidate with no sandbox surfaces the API's 503, never a fabricated run", async ({
  page,
}) => {
  await page.goto("/evals/improvement");
  await expect(page.getByRole("heading", { name: "Improvement", exact: true })).toBeVisible();

  // The first active capability agent is a real row from capability_agents.list.
  await page.getByLabel("Capability agent").selectOption({ index: 1 });
  await page.getByRole("button", { name: "Start a candidate" }).click();

  // The refusal is the action's own sentence (503 benchmark_sandbox_not_configured in dev).
  await expect(page.getByText(/SANDBOX_NAMESPACE is unset/)).toBeVisible();
  await expect(page.locator("table tbody tr")).toHaveCount(2);
});

test("the improvement screen has no horizontal page scroll at 1280 and 390", async ({ page }) => {
  await page.goto("/evals/improvement");
  await expect(page.getByRole("heading", { name: "Improvement", exact: true })).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `width ${width}`).toBeLessThanOrEqual(0);
  }
});
