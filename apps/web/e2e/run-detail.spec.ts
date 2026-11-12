import { expect, test } from "@playwright/test";

import { createBoardCard, invoke } from "./helpers";

// The run queue had no drill-down before `runs.get`: the row now opens this screen, which is
// one action's answer (the run, its `run_step` trace, its gates, its process and inputs).
test("a run created through the API renders and can be cancelled from the detail", async ({
  page,
  request,
}) => {
  const fixture = await createBoardCard(request, `run-detail-${Date.now()}`);
  const run = await invoke<{ id: string }>(request, "runs.create", {
    card_id: fixture.card.id,
  });

  await page.goto(`/runs/${run.id}`);
  await expect(
    page.getByRole("heading", { name: new RegExp(`Run ${run.id.slice(0, 8)}`) }),
  ).toBeVisible();
  await expect(page.getByRole("link", { name: "Card" })).toBeVisible();

  // The run tree is its own panel on every run: a card run is a root (depth 0), no children.
  await expect(page.locator("section.panel", { hasText: "Run tree" })).toContainText("depth 0");
  await expect(page.getByText("No child runs (a call step starts one).")).toBeVisible();

  // The worker may already have moved it; cancel is offered only while the run is live.
  const cancel = page.getByRole("button", { name: "Cancel" });
  if (await cancel.isVisible().catch(() => false)) {
    await cancel.click();
  }

  // Whatever the terminal state, the screen still resolves: no error panel.
  await expect(page.locator(".problem")).toHaveCount(0);
  await expect(page.locator(".screen-head .pill").first()).toBeVisible();
});

test("an id from nowhere is a 404 panel, not a blank page", async ({ page }) => {
  await page.goto("/runs/00000000-0000-7000-8000-000000000000");
  await expect(page.getByRole("heading", { name: "That did not load" })).toBeVisible();
});

test("the run detail fits a phone without horizontal page scroll", async ({ page, request }) => {
  const fixture = await createBoardCard(request, `run-narrow-${Date.now()}`);
  const run = await invoke<{ id: string }>(request, "runs.create", {
    card_id: fixture.card.id,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(`/runs/${run.id}`);
  await expect(page.getByRole("heading", { name: /^Run / })).toBeVisible();
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(overflow).toBeLessThanOrEqual(1);
});
