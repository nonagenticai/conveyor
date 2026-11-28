import { expect, test } from "@playwright/test";

// Sources & retrieval: the plan gate, a capability-bound query's results, the intake catalogue,
// an empty answer and a read failure.

test.describe.configure({ mode: "serial" });

const CAPABILITY = { id: "00000000-0000-4000-8000-000000001001", key: "hiring", name: "Hiring", status: "active" };
const RESULT = { id: "00000000-0000-4000-8000-000000001002", source: "the hiring SOP", text: "check the requisition first" };
// AssetOut (crates/actions/src/authoring/mod.rs) carries `key`, never `title`/`name`; the old
// fixture invented `title`, which is why the always-empty first column could not be seen (#910).
const ASSET = { id: "00000000-0000-4000-8000-000000001003", key: "the hiring SOP", kind: "sop", status: "draft" };

function json(route: any, body: unknown) {
  return route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ output: body }) });
}

test("without knowledge.query the screen is the plan gate naming it", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => entry.name !== "knowledge.query");
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await page.goto("/knowledge/sources");
  await expect(page.getByRole("heading", { name: "Sources & retrieval is planned" })).toBeVisible();
  await expect(page.getByText(/knowledge\.query is not in this role's manifest/)).toBeVisible();
});

test("a query renders its results and the intake catalogue", async ({ page }) => {
  await page.route("**/api/actions/capabilities.list/invocations", (route: any) => json(route, { items: [CAPABILITY] }));
  await page.route("**/api/actions/knowledge.query/invocations", (route: any) => json(route, { items: [RESULT] }));
  await page.route("**/api/actions/authoring.assets/invocations", (route: any) => json(route, { items: [ASSET] }));
  await page.goto("/knowledge/sources");
  await expect(page.getByRole("heading", { name: "Sources & retrieval", level: 1 })).toBeVisible();
  await page.getByLabel("Knowledge query").fill("requisition");
  await page.getByRole("button", { name: "Query" }).click();
  await expect(page.getByTestId("sources-results")).toContainText("the hiring SOP");
  await expect(page.getByTestId("sources-assets")).toContainText("the hiring SOP");
});

test("an empty answer is the app's own message", async ({ page }) => {
  await page.route("**/api/actions/capabilities.list/invocations", (route: any) => json(route, { items: [CAPABILITY] }));
  await page.route("**/api/actions/knowledge.query/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/authoring.assets/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/knowledge/sources");
  await page.getByLabel("Knowledge query").fill("nothing");
  await page.getByRole("button", { name: "Query" }).click();
  await expect(page.getByText("No knowledge result for this query.")).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route("**/api/actions/capabilities.list/invocations", (route: any) => json(route, { items: [CAPABILITY] }));
  await page.route("**/api/actions/knowledge.query/invocations", (route: any) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({ title: "Internal Server Error", status: 500, detail: "the knowledge read failed" }),
    }),
  );
  await page.route("**/api/actions/authoring.assets/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/knowledge/sources");
  await page.getByLabel("Knowledge query").fill("requisition");
  await page.getByRole("button", { name: "Query" }).click();
  await expect(page.locator(".problem")).toContainText("the knowledge read failed");
});

// #910. `CapabilityListIn` flattens `PageIn`, so the limit must ride at the top level; a nested
// `{ page: { limit } }` is silently ignored and the server's default applies.
test("the capability read sends the flattened paging shape the action accepts", async ({ page }) => {
  let sent: any = null;
  await page.route("**/api/actions/capabilities.list/invocations", async (route: any) => {
    sent = route.request().postDataJSON();
    await json(route, { items: [CAPABILITY] });
  });
  await page.route("**/api/actions/knowledge.query/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/authoring.assets/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/knowledge/sources");
  await expect(page.getByRole("heading", { name: "Sources & retrieval", level: 1 })).toBeVisible();
  expect(sent?.limit).toBe(100);
  expect(sent?.page).toBeUndefined();
});

// #910. `knowledge.query` requires `capability_id`. With `capabilities.list` absent the client has
// no id to send, so it refuses locally instead of submitting a request the server will 422.
test("a query with no capability is refused locally, and nothing is sent", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route: any) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => entry.name !== "capabilities.list");
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  let queried = false;
  await page.route("**/api/actions/knowledge.query/invocations", (route: any) => {
    queried = true;
    return json(route, { items: [] });
  });
  await page.goto("/knowledge/sources");
  await page.getByLabel("Knowledge query").fill("requisition");
  await page.getByRole("button", { name: "Query" }).click();
  await expect(page.getByText("Choose a capability; knowledge.query needs one.")).toBeVisible();
  expect(queried).toBe(false);
});
