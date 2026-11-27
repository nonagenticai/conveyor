import { expect, test } from "@playwright/test";

// Models: the plan gate, the list, the detail with its views, an edit carrying If-Match, the
// empty repository and a read failure.

test.describe.configure({ mode: "serial" });

const MODEL = {
  id: "00000000-0000-4000-8000-000000000801",
  project_id: null,
  language: "UML",
  language_version: "2.5",
  title: "The order fulfilment model",
  description: "The classes and flows behind order fulfilment.",
  head_revision: 5,
  published_revision_id: "r4",
  meta: {},
  version: 6,
  created_at: "2026-11-01T09:00:00Z",
  updated_at: "2026-11-20T09:00:00Z",
};
const VIEW = { name: "Class diagram", kind: "class", status: "generated" };

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
  await page.goto("/design/models");
  await expect(page.getByRole("heading", { name: "Models is planned" })).toBeVisible();
  await expect(page.getByText(/models\.list is not in this role's manifest/)).toBeVisible();
});

test("the list renders each model and its revisions", async ({ page }) => {
  await page.route("**/api/actions/models.list/invocations", (route: any) => json(route, { items: [MODEL] }));
  await page.goto("/design/models");
  await expect(page.getByRole("heading", { name: "Models", level: 1 })).toBeVisible();
  await expect(page.getByTestId("models-list")).toContainText("The order fulfilment model");
  await expect(page.getByTestId("models-list")).toContainText("UML 2.5");
  await expect(page.getByRole("button", { name: "New model" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Import XMI" })).toBeVisible();
});

test("the detail renders the model and its views", async ({ page }) => {
  await page.route("**/api/actions/models.get/invocations", (route: any) => json(route, MODEL));
  await page.route("**/api/actions/model_views.list/invocations", (route: any) => json(route, { items: [VIEW] }));
  await page.goto(`/design/models/${MODEL.id}`);
  await expect(page.getByRole("heading", { name: "The order fulfilment model" })).toBeVisible();
  await expect(page.getByTestId("models-views")).toContainText("Class diagram");
  await expect(page.getByRole("button", { name: "Validate" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Publish" })).toBeVisible();
});

test("an edit carries the row's version as If-Match", async ({ page }) => {
  await page.route("**/api/actions/models.get/invocations", (route: any) => json(route, MODEL));
  await page.route("**/api/actions/model_views.list/invocations", (route: any) => json(route, { items: [] }));
  let sentIfMatch: string | undefined;
  await page.route("**/api/actions/models.update/invocations", async (route: any) => {
    sentIfMatch = route.request().headers()["if-match"];
    await json(route, {});
  });
  await page.goto(`/design/models/${MODEL.id}`);
  await page.getByRole("button", { name: "Edit" }).click();
  const dialog = page.getByRole("dialog", { name: "Edit The order fulfilment model" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Submit" }).click();
  await expect.poll(() => sentIfMatch).toBe("6");
  await expect(dialog).toBeHidden({ timeout: 10_000 });
});

test("the empty list is the app's own message", async ({ page }) => {
  await page.route("**/api/actions/models.list/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/design/models");
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
  await page.goto("/design/models");
  await expect(page.locator(".problem")).toContainText("the models read failed");
});
