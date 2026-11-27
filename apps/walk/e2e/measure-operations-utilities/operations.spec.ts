import { expect, test } from "@playwright/test";

// Operations: the plan gate, the pods/events/CI/deployments/flux sections, a write's sheet, the
// empty pods and one failing section that does not blank the page.

test.describe.configure({ mode: "serial" });

function json(route: any, body: unknown) {
  return route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ output: body }) });
}

test("without ops.pods the screen is the plan gate naming it", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => entry.name !== "ops.pods");
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await page.goto("/operations");
  await expect(page.getByRole("heading", { name: "Operations is planned" })).toBeVisible();
  await expect(page.getByText(/ops\.pods is not in this role's manifest/)).toBeVisible();
});

test("the operations sections render", async ({ page }) => {
  await page.route("**/api/actions/ops.pods/invocations", (route: any) => json(route, { items: [{ metadata: { name: "conveyor-abc" }, status: { phase: "Running" } }] }));
  await page.route("**/api/actions/ops.events/invocations", (route: any) => json(route, { items: [{ reason: "Pulled", message: "the image was pulled" }] }));
  await page.route("**/api/actions/ops.ci_status/invocations", (route: any) => json(route, { runs: [] }));
  await page.route("**/api/actions/ops.deployments/invocations", (route: any) => json(route, { items: [{ metadata: { name: "conveyor" }, status: { readyReplicas: 2, replicas: 2 } }] }));
  await page.route("**/api/actions/ops.flux_status/invocations", (route: any) => json(route, { items: [{ metadata: { name: "conveyor" }, status: { conditions: [{ type: "Ready", status: "True", message: "ok" }] } }] }));
  await page.goto("/operations");
  await expect(page.getByRole("heading", { name: "Operations", level: 1 })).toBeVisible();
  await expect(page.getByTestId("operations-pods")).toContainText("conveyor-abc");
  await expect(page.getByTestId("operations-events")).toContainText("Pulled");
  await expect(page.getByTestId("operations-flux")).toContainText("conveyor");
  await expect(page.getByRole("button", { name: "Open an incident" })).toBeVisible();
});

test("a write opens its sheet", async ({ page }) => {
  await page.route("**/api/actions/ops.pods/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/ops.events/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/ops.ci_status/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/ops.deployments/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/ops.flux_status/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/operations");
  await page.getByRole("button", { name: "Open an incident" }).click();
  const dialog = page.getByRole("dialog", { name: "Open an incident" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Submit" })).toBeVisible();
});

test("the empty pods is the app's own message", async ({ page }) => {
  await page.route("**/api/actions/ops.pods/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/ops.events/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/ops.ci_status/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/ops.deployments/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/ops.flux_status/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/operations");
  await expect(page.getByText("No pod in the conveyor namespace.")).toBeVisible();
});

test("one failing section does not blank the page", async ({ page }) => {
  await page.route("**/api/actions/ops.pods/invocations", (route: any) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({ title: "Internal Server Error", status: 500, detail: "the pods read failed" }),
    }),
  );
  await page.route("**/api/actions/ops.events/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/ops.ci_status/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/ops.deployments/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/ops.flux_status/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/operations");
  await expect(page.locator(".problem")).toContainText("the pods read failed");
});
