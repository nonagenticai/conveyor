import { expect, test } from "./fixture";

import { authFor, createBoardCard } from "./helpers";

// T18-1 P2/P3: Portfolio reads the tenant's rows; the delivery view renders L1–L4; a spend row
// with no priced call is `—*`, never `$0`.

test("the portfolio renders the tenant's in-flight set, never a fixture", async ({
  page,
  request,
}) => {
  const title = `e2e portfolio ${Date.now()}`;
  await createBoardCard(request, title);

  const response = await request.get("/api/projects?limit=200", { headers: authFor() });
  expect(response.ok()).toBe(true);
  const document = (await response.json()) as { items: { name: string }[] };
  const names = document.items.map((project) => project.name);
  expect(names).toContain(title);

  await page.goto("/portfolio");
  await expect(page.getByRole("heading", { name: "Portfolio", exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { name: "L1 · Portfolio" })).toBeVisible();

  const table = page.locator("table.table").first();
  const row = table.locator("tr", { hasText: title });
  await expect(row).toBeVisible();
  await expect(row.getByText("no dated activity")).toBeVisible();
  await expect(row.getByText("—*")).toBeVisible();

  // Every rendered project row names a project the API returned: no fixture row survives.
  const rendered = await table
    .locator("tbody tr")
    .evaluateAll((rows) => rows.map((row) => (row.children[1] as HTMLElement).innerText.trim()));
  for (const name of rendered) {
    expect(names).toContain(name);
  }

  await expect(page.getByRole("heading", { name: "L2 · Programme" })).toBeVisible();
});

test("the delivery view renders L1-L4 and the work package's execution", async ({
  page,
  request,
}) => {
  const title = `e2e delivery ${Date.now()}`;
  const fixture = await createBoardCard(request, title);

  await page.goto("/portfolio");
  const row = page.locator("table.table").first().locator("tr", { hasText: title });
  await row.getByRole("button", { name: "Open" }).click();

  await expect(page.getByRole("heading", { name: "L3 · Project" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "L4 · Work package" })).toBeVisible();
  // The work package's own execution link is the cockpit route for THAT card; the board may
  // carry other cards, so target the fixture's href rather than the first link.
  await expect(page.locator(`a[href="/card/${fixture.card.id}"]`)).toBeVisible();
});

test("an unpriced row is marked", async ({ page, request }) => {
  const title = `e2e unpriced ${Date.now()}`;
  await createBoardCard(request, title);

  await page.goto("/portfolio");
  const row = page.locator("table.table").first().locator("tr", { hasText: title });
  await expect(row.getByText("—*")).toBeVisible();
  await expect(row.getByText("no priced call")).toBeVisible();
  await expect(row.getByText("$0")).toHaveCount(0);
});
