import { expect, test } from "@playwright/test";

// System architecture: the plan gate, the models with their revisions, the views generated from
// the head model, the generate sheet, the empty repository and a read failure.

test.describe.configure({ mode: "serial" });

const MODEL = {
  id: "00000000-0000-4000-8000-000000000601",
  project_id: null,
  language: "ArchiMate",
  language_version: "3.2",
  title: "The estate capability model",
  description: "Capabilities, their actors and the flows between them.",
  head_revision: 7,
  published_revision_id: "r6",
  meta: {},
  version: 3,
  created_at: "2026-11-01T09:00:00Z",
  updated_at: "2026-11-20T09:00:00Z",
};
const VIEW = { name: "Capability map", kind: "capability", status: "generated" };

function json(route: any, body: unknown) {
  return route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ output: body }) });
}

test("without models.list the screen is the plan gate naming it", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => entry.name !== "models.list");
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await page.goto("/architecture");
  await expect(page.getByRole("heading", { name: "Architecture is planned" })).toBeVisible();
  await expect(page.getByText(/models\.list is not in this role's manifest/)).toBeVisible();
});

test("the models and their generated views render", async ({ page }) => {
  await page.route("**/api/actions/models.list/invocations", (route: any) => json(route, { items: [MODEL] }));
  await page.route("**/api/actions/model_views.list/invocations", (route: any) => json(route, { items: [VIEW] }));
  await page.goto("/architecture");
  await expect(page.getByRole("heading", { name: "System architecture", level: 1 })).toBeVisible();
  await expect(page.getByTestId("architecture-models")).toContainText("The estate capability model");
  await expect(page.getByTestId("architecture-models")).toContainText("ArchiMate");
  await expect(page.getByTestId("architecture-models")).toContainText("7");
  await expect(page.getByTestId("architecture-views")).toContainText("Capability map");
  await expect(page.getByRole("button", { name: "Generate a view" })).toBeVisible();
});

test("the view-generate write opens its sheet", async ({ page }) => {
  await page.route("**/api/actions/models.list/invocations", (route: any) => json(route, { items: [MODEL] }));
  await page.route("**/api/actions/model_views.list/invocations", (route: any) => json(route, { items: [VIEW] }));
  await page.goto("/architecture");
  await page.getByRole("button", { name: "Generate a view" }).click();
  const dialog = page.getByRole("dialog", { name: "Generate a view" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Submit" })).toBeVisible();
});

test("the empty repository is the app's own message", async ({ page }) => {
  await page.route("**/api/actions/models.list/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/architecture");
  await expect(page.getByText("No model yet. This is the empty state, not an error.")).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route("**/api/actions/models.list/invocations", (route: any) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({ title: "Internal Server Error", status: 500, detail: "the models read failed" }),
    }),
  );
  await page.goto("/architecture");
  await expect(page.locator(".problem")).toContainText("the models read failed");
});
