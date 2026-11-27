import { expect, test } from "@playwright/test";

// Procedures: the plan gate when procedures.list is absent, the family with its processes,
// capabilities and gates, the draft edit carrying its lock version as If-Match, the ingest verbs
// and the empty/error/signed-out states.

test.describe.configure({ mode: "serial" });

const PROCEDURE = {
  id: "00000000-0000-4000-8000-000000000101",
  key: "hire-someone",
  title: "Hire someone",
  description: "From an approved requisition to a signed offer.",
  status: "draft",
  version: 2,
  lock_version: 3,
  definition_of_ready: {},
  definition_of_done: {},
  acceptance_criteria: {},
  inspection: {},
  outputs: {},
  knowledge_pack_ref: "",
};
const PROCESS = {
  id: "00000000-0000-4000-8000-000000000102",
  key: "intake-to-clear",
  title: "Intake to clear",
  status: "published",
  version: 1,
  lock_version: 1,
};
const CAPABILITY = { key: "hiring", name: "Hiring", status: "active" };
const APPROVAL = { gate: "procedures.publish", kind: "process", status: "pending" };

function json(route: any, body: unknown) {
  return route.fulfill({
    status: 200,
    contentType: "application/json",
    body: JSON.stringify({ output: body }),
  });
}

async function doubleLists(page: any) {
  await page.route("**/api/actions/procedures.list/invocations", (route: any) =>
    json(route, { items: [PROCEDURE] }),
  );
  await page.route("**/api/actions/processes.list/invocations", (route: any) =>
    json(route, { items: [PROCESS] }),
  );
  await page.route("**/api/actions/capabilities.list/invocations", (route: any) =>
    json(route, { items: [CAPABILITY] }),
  );
  await page.route("**/api/actions/approvals.list/invocations", (route: any) =>
    json(route, { items: [APPROVAL] }),
  );
}

test("without procedures.list the screen is the plan gate naming it", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => entry.name !== "procedures.list");
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await page.goto("/procedures");
  await expect(page.getByRole("heading", { name: "Procedures is planned" })).toBeVisible();
  await expect(page.getByText(/procedures\.list is not in this role's manifest/)).toBeVisible();
});

test("the families, capabilities and gates render", async ({ page }) => {
  await doubleLists(page);
  await page.goto("/procedures");
  await expect(page.getByRole("heading", { name: "Procedures", level: 1 })).toBeVisible();
  await expect(page.getByTestId("procedures-list")).toContainText("Hire someone");
  await expect(page.getByTestId("procedures-list")).toContainText("draft");
  await expect(page.getByTestId("procedures-processes")).toContainText("Intake to clear");
  await expect(page.getByTestId("procedures-capabilities")).toContainText("Hiring");
  await expect(page.getByTestId("procedures-approvals")).toContainText("pending");
  await expect(page.getByRole("button", { name: "New procedure" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Ingest SOP" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Ingest process" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Ingest RDF" })).toBeVisible();
});

test("a draft edit carries the row's lock version as If-Match", async ({ page }) => {
  await doubleLists(page);
  let sentIfMatch: string | undefined;
  await page.route("**/api/actions/procedures.update/invocations", async (route: any) => {
    sentIfMatch = route.request().headers()["if-match"];
    await json(route, {});
  });
  await page.goto("/procedures");
  await page.getByTestId("procedures-list").getByRole("button", { name: "Edit" }).click();
  const dialog = page.getByRole("dialog", { name: "Edit Hire someone" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Submit" }).click();
  await expect.poll(() => sentIfMatch).toBe("3");
  await expect(dialog).toBeHidden({ timeout: 10_000 });
});

test("the empty family is the app's own message", async ({ page }) => {
  await page.route("**/api/actions/procedures.list/invocations", (route: any) =>
    json(route, { items: [] }),
  );
  await page.route("**/api/actions/processes.list/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/capabilities.list/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/approvals.list/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/procedures");
  await expect(page.getByText("No procedure yet. This is the empty state, not an error.")).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route("**/api/actions/procedures.list/invocations", (route: any) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({ title: "Internal Server Error", status: 500, detail: "the procedures read failed" }),
    }),
  );
  await page.goto("/procedures");
  await expect(page.locator(".problem")).toContainText("the procedures read failed");
});

test("a signed-out visit is the sign-in gate", async ({ page }) => {
  await page.goto("/procedures");
  await page.evaluate(() => {
    document.cookie = "conveyor_dev=; Max-Age=0; Path=/";
  });
  await page.goto("/procedures");
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
});
