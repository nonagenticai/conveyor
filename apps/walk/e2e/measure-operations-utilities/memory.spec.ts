import { expect, test } from "@playwright/test";

// Memory: the plan gate, a recall's hot rows and digest hits, the empty state and a read failure.
// The lesson family is propose/review/promote; there is no list read to assert.

test.describe.configure({ mode: "serial" });

const CARD = "00000000-0000-4000-8000-000000003001";
const HOT = { id: "00000000-0000-4000-8000-000000003002", card_id: CARD, kind: "fact", source: "agent", body: "the requisition must be signed before the order" };
const COLD = { id: "00000000-0000-4000-8000-000000003003", source: "digest", text: "a signed requisition precedes every order" };

function json(route: any, body: unknown) {
  return route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ output: body }) });
}

test("without memory.recall the screen is the plan gate naming it", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => entry.name !== "memory.recall");
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await page.goto("/knowledge/memory");
  await expect(page.getByRole("heading", { name: "Memory is planned" })).toBeVisible();
  await expect(page.getByText(/memory\.recall is not in this role's manifest/)).toBeVisible();
});

test("a recall renders the hot rows and the digest hits", async ({ page }) => {
  await page.route("**/api/actions/memory.recall/invocations", (route: any) =>
    json(route, { card_id: CARD, hot: [HOT], cold: [COLD], degraded: false, warnings: [] }),
  );
  await page.goto("/knowledge/memory");
  await expect(page.getByRole("heading", { name: "Memory", level: 1 })).toBeVisible();
  await page.getByLabel("Work package").fill(CARD);
  await page.getByRole("button", { name: "Recall" }).click();
  await expect(page.getByTestId("memory-hot")).toContainText("the requisition must be signed");
  await expect(page.getByTestId("memory-cold")).toContainText("a signed requisition precedes every order");
});

test("a recall with no row is the empty state", async ({ page }) => {
  await page.route("**/api/actions/memory.recall/invocations", (route: any) =>
    json(route, { card_id: CARD, hot: [], cold: [], degraded: false, warnings: [] }),
  );
  await page.goto("/knowledge/memory");
  await page.getByLabel("Work package").fill(CARD);
  await page.getByRole("button", { name: "Recall" }).click();
  await expect(page.getByText("No memory row for this work package yet.")).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route("**/api/actions/memory.recall/invocations", (route: any) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({ title: "Internal Server Error", status: 500, detail: "the memory read failed" }),
    }),
  );
  await page.goto("/knowledge/memory");
  await page.getByLabel("Work package").fill(CARD);
  await page.getByRole("button", { name: "Recall" }).click();
  await expect(page.locator(".problem")).toContainText("the memory read failed");
});
