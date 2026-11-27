import { expect, test } from "@playwright/test";

// Feedback: the plan gate, the tenant's submissions, the write sheet, the empty state and a read
// failure. The screen is the tenant's own; the operator's cross-tenant view is not in the walk.

test.describe.configure({ mode: "serial" });

const ROW = {
  id: "00000000-0000-4000-8000-000000002001",
  kind: "bug",
  title: "the board froze",
  body: "opening the board with twenty cards froze the tab",
  route: "/boards",
  status: "open",
  created_at: "2026-11-27T20:00:00Z",
};

function json(route: any, body: unknown) {
  return route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ output: body }) });
}

test("without feedback.list the screen is the plan gate naming it", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => entry.name !== "feedback.list");
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await page.goto("/feedback");
  await expect(page.getByRole("heading", { name: "Feedback is planned" })).toBeVisible();
  await expect(page.getByText(/feedback\.list is not in this role's manifest/)).toBeVisible();
});

test("the tenant's submissions render", async ({ page }) => {
  await page.route("**/api/actions/feedback.list/invocations", (route: any) => json(route, { items: [ROW] }));
  await page.goto("/feedback");
  await expect(page.getByRole("heading", { name: "Feedback", level: 1 })).toBeVisible();
  const table = page.getByTestId("feedback-rows");
  await expect(table).toContainText("the board froze");
  await expect(table).toContainText("/boards");
  await expect(table).toContainText("open");
});

test("the write opens its sheet", async ({ page }) => {
  await page.route("**/api/actions/feedback.list/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/feedback");
  await page.getByRole("button", { name: "Send feedback" }).click();
  const dialog = page.getByRole("dialog", { name: "Send feedback" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Submit" })).toBeVisible();
});

test("an empty list is the app's own message", async ({ page }) => {
  await page.route("**/api/actions/feedback.list/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/feedback");
  await expect(page.getByText("No feedback yet. Yours will appear here.")).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route("**/api/actions/feedback.list/invocations", (route: any) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({ title: "Internal Server Error", status: 500, detail: "the feedback read failed" }),
    }),
  );
  await page.goto("/feedback");
  await expect(page.locator(".problem")).toContainText("the feedback read failed");
});
