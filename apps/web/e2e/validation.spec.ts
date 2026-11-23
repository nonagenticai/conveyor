import { expect, test } from "./fixture";

import { createBoardCard } from "./helpers";

// #563 T18-2 — Validation: the measured programme, read from the tenant's own project rows.

test("validation renders the measured programme", async ({ page, request }) => {
  const title = `e2e validation ${Date.now()}`;
  await createBoardCard(request, title);

  await page.goto("/validation");
  const table = page.locator('[data-testid="validation-table"]');
  await expect(table).toBeVisible();
  await expect(table).toContainText(title);
  // Every row carries the project's own status, not a fixture.
  await expect(table.locator("tbody tr").first()).toContainText(/draft|active|closed|paused/i);
});
