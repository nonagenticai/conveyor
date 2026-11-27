import { expect, test } from "@playwright/test";

// Usage: the plan gate, the three measurement sections, one failing section that does not blank
// the page.

test.describe.configure({ mode: "serial" });

function json(route: any, body: unknown) {
  return route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ output: body }) });
}

test("without metrics.usage the screen is the plan gate naming it", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => entry.name !== "metrics.usage");
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await page.goto("/measure/usage");
  await expect(page.getByRole("heading", { name: "Usage is planned" })).toBeVisible();
  await expect(page.getByText(/metrics\.usage is not in this role's manifest/)).toBeVisible();
});

test("the usage, billing and inference sections render", async ({ page }) => {
  await page.route("**/api/actions/metrics.usage/invocations", (route: any) => json(route, { items: [{ key: "cards.created", value: 12 }] }));
  await page.route("**/api/actions/billing.summary/invocations", (route: any) => json(route, { total_usd: 4.2, items: [{ project: "conveyor", usd: 4.2 }] }));
  await page.route("**/api/actions/inference_usage.report/invocations", (route: any) => json(route, { items: [{ model: "opus", tokens: 1000 }] }));
  await page.goto("/measure/usage");
  await expect(page.getByRole("heading", { name: "Usage", level: 1 })).toBeVisible();
  await expect(page.getByTestId("usage-metrics")).toContainText("cards.created");
  await expect(page.getByTestId("usage-billing")).toBeVisible();
  await expect(page.getByTestId("usage-inference")).toBeVisible();
});

test("one failing section does not blank the page", async ({ page }) => {
  await page.route("**/api/actions/metrics.usage/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/billing.summary/invocations", (route: any) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({ title: "Internal Server Error", status: 500, detail: "the billing read failed" }),
    }),
  );
  await page.route("**/api/actions/inference_usage.report/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/measure/usage");
  await expect(page.locator(".problem")).toContainText("the billing read failed");
  await expect(page.getByTestId("usage-metrics")).toBeVisible();
});
