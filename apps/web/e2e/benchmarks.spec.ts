import { expect, test } from "@playwright/test";

// The seeded suite is the embedded verified50 artifact; the two runs are controls (`gold`
// resolves all eight seeded case rows, `empty` none). S18's conveyor arm is measurable now;
// the baseline arm is still refused by the action, and the screen must say so.
test("the suite and its control runs render; conveyor is offered and baseline is refused", async ({
  page,
}) => {
  // `/evals/benchmarks` must win over the `/evals/:eval_id` route.
  await page.goto("/evals/benchmarks");
  await expect(page.getByRole("heading", { name: "Benchmarks" })).toBeVisible();
  await expect(page.getByText("verified50-v1").first()).toBeVisible();
  await expect(page.getByText("The conveyor arm measures one pinned capability agent; the baseline arm is still refused by the action (409 arm_not_ready).")).toBeVisible();

  const arm = page.getByLabel("Arm");
  await expect(arm.locator('option[value="baseline"]')).toBeDisabled();
  await expect(arm.locator('option[value="conveyor"]')).toBeEnabled();

  // Two control runs are listed for the selected suite.
  await expect(page.locator("section.panel", { hasText: "Runs" }).locator("tbody tr")).toHaveCount(2);
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("the run detail shows the paired matrix and calls a control comparison what it is", async ({
  page,
}) => {
  await page.goto("/evals/benchmarks");
  await page.getByRole("link", { name: "gold", exact: true }).click();
  await expect(page).toHaveURL(/\/evals\/benchmarks\/run\//);

  await expect(page.locator(".tile", { hasText: "This run" }).locator(".tv")).toHaveText("8 / 8");

  // The pair is gold vs empty: a valid comparison of two controls, and the screen says so
  // before anyone reads the delta as a subject improvement.
  await expect(page.getByText("Both runs are controls: this is a suite check, not a subject comparison.")).toBeVisible();
  await expect(page.getByRole("heading", { name: "Paired outcomes" })).toBeVisible();

  // The case table joins the two runs on the instance id.
  await expect(page.locator(".panel", { hasText: "Cases" }).locator("tbody tr")).toHaveCount(8);
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("the screen fits a phone without horizontal page scroll", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/evals/benchmarks");
  await expect(page.getByRole("heading", { name: "Benchmarks" })).toBeVisible();
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(overflow).toBeLessThanOrEqual(1);
});
