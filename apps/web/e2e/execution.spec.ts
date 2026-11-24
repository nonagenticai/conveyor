import { expect, test } from "./fixture";

import { createBoardCard, invoke } from "./helpers";

// T18-3 P3: the execution room reads the tenant's runs and the run's provenance (`runs.get`).
// The executor kind, the job binding / release hash pin and the tool calls are not exposed by the
// read model yet; the screen names those gaps instead of inventing columns (T0 · T4 · S21, T13).

test("the execution room reads the run's provenance and names the missing ledger", async ({
  page,
  request,
}) => {
  const fixture = await createBoardCard(request, `execution-${Date.now()}`);
  const run = await invoke<{ id: string }>(request, "runs.create", {
    card_id: fixture.card.id,
  });
  const short = run.id.slice(0, 8);

  await page.goto("/execution");
  await expect(page.getByRole("heading", { name: "Execution room" })).toBeVisible();

  // Two runs can share the first eight characters of their ULID; address the row by the full
  // run id in its link instead.
  const row = page
    .getByTestId("execution-runs")
    .locator("tr", { has: page.locator(`a[href="/runs/${run.id}"]`) });
  await expect(row).toBeVisible();
  await expect(row.getByRole("link", { name: "Open" })).toHaveAttribute(
    "href",
    `/runs/${run.id}`,
  );

  await row.getByRole("button", { name: short }).click();
  await expect(page.getByTestId("execution-detail")).toBeVisible();
  await expect(page.getByText("Steps", { exact: true })).toBeVisible();
  await expect(page.getByText("No step has run yet.")).toBeVisible();
  await expect(page.getByText("Explain context", { exact: true })).toBeVisible();

  // The gaps are named, not faked: the executor/pin read model and T13's tool ledger.
  await expect(page.getByText(/executor kind/i)).toBeVisible();
  await expect(page.getByText(/job binding/i)).toBeVisible();
  await expect(page.getByText(/T13/)).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
});
