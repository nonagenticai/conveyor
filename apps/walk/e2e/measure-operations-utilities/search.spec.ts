import { expect, test } from "@playwright/test";

// Search: the plan gate, a query's results, the empty answer and a read failure.

test.describe.configure({ mode: "serial" });

const RESULT = { type: "card", title: "The door holds", id: "00000000-0000-4000-8000-000000000b01" };

function json(route: any, body: unknown) {
  return route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ output: body }) });
}

test("without search.query the screen is the plan gate naming it", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => entry.name !== "search.query");
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await page.goto("/search");
  await expect(page.getByRole("heading", { name: "Search is planned" })).toBeVisible();
  await expect(page.getByText(/search\.query is not in this role's manifest/)).toBeVisible();
});

test("a query renders its results", async ({ page }) => {
  await page.route("**/api/actions/search.query/invocations", (route: any) => json(route, { items: [RESULT] }));
  await page.goto("/search");
  await expect(page.getByRole("heading", { name: "Search", level: 1 })).toBeVisible();
  await page.getByLabel("Search query").fill("door");
  await page.getByRole("button", { name: "Search" }).click();
  await expect(page.getByTestId("search-results")).toContainText("The door holds");
  await expect(page.getByTestId("search-results")).toContainText("card");
});

test("an empty answer is the app's own message", async ({ page }) => {
  await page.route("**/api/actions/search.query/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/search");
  await page.getByLabel("Search query").fill("nothing");
  await page.getByRole("button", { name: "Search" }).click();
  await expect(page.getByText("No result for this query.")).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route("**/api/actions/search.query/invocations", (route: any) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({ title: "Internal Server Error", status: 500, detail: "the search failed" }),
    }),
  );
  await page.goto("/search");
  await page.getByLabel("Search query").fill("door");
  await page.getByRole("button", { name: "Search" }).click();
  await expect(page.locator(".problem")).toContainText("the search failed");
});
