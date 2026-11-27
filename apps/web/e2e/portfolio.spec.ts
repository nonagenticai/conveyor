import { expect, test, type Page } from "./fixture";

import { authFor, createBoardCard, invoke, isDevAuth } from "./helpers";

const baseURL = process.env.BASE_URL ?? "http://127.0.0.1:8931";

/** Swap the dev cookie. The file's storageState is alice (admin of dev-a). */
async function as(page: Page, login: string) {
  const host = new URL(baseURL).hostname;
  await page.context().addCookies([
    { name: "conveyor_dev", value: login, domain: host, path: "/" },
  ]);
}

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
  const fixture = await createBoardCard(request, title);

  // The page composes spend from its own report load; prove the project has no priced or
  // unpriced call on the server first, so the assertion is not a race with that snapshot.
  const until = new Date().toISOString();
  const since = new Date(Date.now() - 30 * 24 * 60 * 60 * 1000).toISOString();
  await expect
    .poll(
      async () => {
        const report = await invoke<{
          rows: { bucket: string; priced_calls?: number; unpriced_calls?: number }[];
        }>(request, "inference_usage.report", {
          since,
          until,
          group_by: "project",
        });
        const entry = report.rows.find((row) => row.bucket === fixture.projectId);
        return entry ? (entry.priced_calls ?? 0) + (entry.unpriced_calls ?? 0) : 0;
      },
      { timeout: 30_000 },
    )
    .toBe(0);

  await page.goto("/portfolio");
  // The row carries its project id, so the assertion cannot drift to another project with a
  // similar title.
  const row = page.locator(`tr[data-project-id="${fixture.projectId}"]`);
  await expect(row.getByText("—*")).toBeVisible();
  // The project can already have unpriced calls from the board's start-process jobs; the honest
  // note is either "no priced call" (zero calls) or "N unpriced". The invariant is the marker
  // and the absence of a dollar figure.
  await expect(row.getByText(/no priced call|\d+ unpriced/)).toBeVisible();
  await expect(row.getByText("$0")).toHaveCount(0);
});

// #848 — the T18 §10 remainder: empty, error and one server refusal beside the happy path.
//
// The empty state is `fr-test`'s kata/execution tenant, which holds no project and stays empty
// for the whole suite — not dev-b, which processes.spec.ts:138 fills with a board card. The
// refusal is the tenant boundary: Portfolio has no write surface to refuse, and the server's
// scoping is the refusal this destination makes — another tenant's project id is a 404.
test("an empty tenant says so, never a fixture", async ({ page }) => {
  test.skip(!isDevAuth(), "the deployed suite has one principal; fr-test is a dev seed");
  await as(page, "fr-test");
  await page.goto("/portfolio");
  await expect(page.getByRole("heading", { name: "Portfolio", exact: true })).toBeVisible();
  await expect(page.getByText("No projects in this tenant yet.")).toBeVisible();
  await expect(page.locator("table.table")).toHaveCount(0);
});

test("a failed portfolio read is the error panel, with a retry", async ({ page }) => {
  await page.route(/\/api\/projects\?limit=200/, (route) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({ detail: "forced by the spec" }),
    }),
  );
  await page.goto("/portfolio");
  const panel = page.locator("div.panel.problem[role=alert]");
  await expect(panel.getByRole("heading", { name: "That did not load" })).toBeVisible();
  await expect(panel).toContainText("forced by the spec");
  await expect(panel.getByRole("button", { name: "Try again" })).toBeVisible();
});

test("another tenant's project is refused by the server", async ({ request }) => {
  const fixture = await createBoardCard(request, `e2e tenant refusal ${Date.now()}`);
  const response = await request.get(`/api/projects/${fixture.projectId}`, {
    headers: authFor("bob"),
  });
  expect(response.status()).toBe(404);
  expect(await response.text()).toContain("no such project in this tenant");
});
