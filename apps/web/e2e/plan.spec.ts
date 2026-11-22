import { expect, test } from "./fixture";

// SPEC_V2 §10.5 — the live Plan destination. Five reads against the selected project:
// projects.list, wbs.list, baselines.list, registers.list and project_dependencies.list.
// Acceptance is evidence, not a checkbox; the screen only reads.

test("the plan destination reads the selected project's plan spine", async ({ page }) => {
  await page.goto("/plan");
  await expect(page.getByRole("heading", { name: "Plan", exact: true })).toBeVisible();

  await expect(page.locator(".problem")).toHaveCount(0);
  await expect(page.getByLabel("Project")).toBeVisible();
  await expect(page.locator(".tile")).toHaveCount(4);

  await expect(page.locator("table.table")).toHaveCount(4);
  for (const heading of ["WBS", "Baselines", "Registers", "Dependencies"]) {
    await expect(page.getByRole("heading", { name: heading, exact: true })).toBeVisible();
  }
});

test("the plan screen has no horizontal page scroll at 1280 and 390", async ({ page }) => {
  await page.goto("/plan");
  await expect(page.getByRole("heading", { name: "Plan", exact: true })).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `width ${width}`).toBeLessThanOrEqual(0);
  }
});
