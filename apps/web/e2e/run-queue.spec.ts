import { expect, test } from "@playwright/test";

test("the run queue filters by state", async ({ page }) => {
  await page.goto("/runs");
  await expect(page.getByRole("heading", { name: "Runs" })).toBeVisible();

  // The shell also has a tenant <select>; this is the screen's own filter.
  const filter = page.locator(".screen-head select");
  await filter.selectOption("failed");
  await expect(filter).toHaveValue("failed");

  // Either the empty state, or only failed rows. A stale row from another filter is a bug.
  await expect(
    page
      .getByText("No runs in this filter.")
      .or(page.locator(".pill", { hasText: /failed/i }).first()),
  ).toBeVisible({ timeout: 10_000 });
  await expect
    .poll(async () => {
      const pills = await page.locator(".pill").allInnerTexts();
      return pills.every((pill) => pill.toLowerCase().includes("failed"));
    })
    .toBe(true);
  await expect(page.locator(".problem")).toHaveCount(0);
});
