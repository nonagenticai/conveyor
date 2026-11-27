import { expect, test } from "@playwright/test";

// Metering: the plan gate, the billing, usage and inference sections, one failing section that
// does not blank the page.

test.describe.configure({ mode: "serial" });

function json(route: any, body: unknown) {
  return route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ output: body }) });
}

test("without billing.summary the screen is the plan gate naming it", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => entry.name !== "billing.summary");
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await page.goto("/metering");
  await expect(page.getByRole("heading", { name: "Metering is planned" })).toBeVisible();
  await expect(page.getByText(/billing\.summary is not in this role's manifest/)).toBeVisible();
});

test("the metering sections render", async ({ page }) => {
  await page.route("**/api/actions/billing.summary/invocations", (route: any) => json(route, { total_usd: 4.2, items: [{ project: "conveyor", usd: 4.2 }] }));
  await page.route("**/api/actions/metrics.usage/invocations", (route: any) => json(route, { items: [{ key: "cards.created", value: 12 }] }));
  await page.route("**/api/actions/inference_usage.report/invocations", (route: any) => json(route, { items: [{ model: "opus", tokens: 1000 }] }));
  await page.goto("/metering");
  await expect(page.getByRole("heading", { name: "Metering", level: 1 })).toBeVisible();
  await expect(page.getByTestId("metering-billing")).toContainText("conveyor");
  await expect(page.getByTestId("metering-usage")).toContainText("cards.created");
  await expect(page.getByTestId("metering-inference")).toBeVisible();
});

test("one failing section does not blank the page", async ({ page }) => {
  await page.route("**/api/actions/billing.summary/invocations", (route: any) => json(route, { total_usd: 4.2, items: [] }));
  await page.route("**/api/actions/metrics.usage/invocations", (route: any) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({ title: "Internal Server Error", status: 500, detail: "the usage read failed" }),
    }),
  );
  await page.route("**/api/actions/inference_usage.report/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/metering");
  await expect(page.locator(".problem")).toContainText("the usage read failed");
  await expect(page.getByTestId("metering-billing")).toBeVisible();
});
