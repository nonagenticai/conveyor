import { expect, test } from "./fixture";

// SPEC_V2 §10.5 — the live Improve destination. `metrics.flow` is read at call grain; a figure
// without a source ledger is listed unavailable with its reason, never rendered as zero. The
// kata queue is honestly planned until the kata.* family (T5) lands.

test("the improve destination reads the flow figures and says what is unavailable", async ({
  page,
}) => {
  await page.goto("/improve");
  await expect(page.getByRole("heading", { name: "Improve", exact: true })).toBeVisible();

  await expect(page.locator(".problem")).toHaveCount(0);
  await expect(page.locator(".tile")).toHaveCount(4);

  // The unavailable list is a real panel either way: a table when a ledger is missing, the
  // honest empty sentence when every figure has one.
  await expect(page.getByRole("heading", { name: "Unavailable figures", exact: true })).toBeVisible();

  // The kata queue is not faked: it is a Planned panel naming the actions that will open it.
  await expect(page.getByRole("heading", { name: "Kata queue", exact: true })).toBeVisible();
  await expect(page.getByText("The kata queue is planned")).toBeVisible();
});

test("the improve screen has no horizontal page scroll at 1280 and 390", async ({ page }) => {
  await page.goto("/improve");
  await expect(page.getByRole("heading", { name: "Improve", exact: true })).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `width ${width}`).toBeLessThanOrEqual(0);
  }
});
