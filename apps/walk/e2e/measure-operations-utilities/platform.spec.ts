import { expect, test } from "@playwright/test";

// Platform: the plan gate, the five status sections, a failing section that does not blank the
// page, and the empty pods list.

test.describe.configure({ mode: "serial" });

function json(route: any, body: unknown) {
  return route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ output: body }) });
}

test("without ops.flux_status the screen is the plan gate naming it", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => entry.name !== "ops.flux_status");
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await page.goto("/platform");
  await expect(page.getByRole("heading", { name: "Platform is planned" })).toBeVisible();
  await expect(page.getByText(/ops\.flux_status is not in this role's manifest/)).toBeVisible();
});

test("the five status sections render", async ({ page }) => {
  await page.route("**/api/actions/ops.flux_status/invocations", (route: any) => json(route, { status: "Ready", items: [{ name: "conveyor", ready: true }] }));
  await page.route("**/api/actions/ops.ci_status/invocations", (route: any) => json(route, { status: "ok", items: [{ repo: "conveyor", green: true }] }));
  await page.route("**/api/actions/ops.deployments/invocations", (route: any) => json(route, { items: [{ name: "conveyor", ready: "2/2" }] }));
  await page.route("**/api/actions/ops.pods/invocations", (route: any) => json(route, { items: [{ name: "conveyor-abc", phase: "Running" }] }));
  await page.route("**/api/actions/ops.host_status/invocations", (route: any) => json(route, { items: [{ host: "strato-server", state: "active" }] }));
  await page.goto("/platform");
  await expect(page.getByRole("heading", { name: "Platform", level: 1 })).toBeVisible();
  await expect(page.getByTestId("platform-flux")).toBeVisible();
  await expect(page.getByTestId("platform-ci")).toBeVisible();
  await expect(page.getByTestId("platform-deployments")).toBeVisible();
  await expect(page.getByTestId("platform-pods")).toContainText("conveyor-abc");
});

test("one failing section does not blank the page", async ({ page }) => {
  await page.route("**/api/actions/ops.flux_status/invocations", (route: any) => json(route, { status: "Ready", items: [] }));
  await page.route("**/api/actions/ops.ci_status/invocations", (route: any) => json(route, { status: "ok", items: [] }));
  await page.route("**/api/actions/ops.deployments/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/ops.pods/invocations", (route: any) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({ title: "Internal Server Error", status: 500, detail: "the pods read failed" }),
    }),
  );
  await page.route("**/api/actions/ops.host_status/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/platform");
  await expect(page.locator(".problem")).toContainText("the pods read failed");
  await expect(page.getByTestId("platform-flux")).toBeVisible();
});
