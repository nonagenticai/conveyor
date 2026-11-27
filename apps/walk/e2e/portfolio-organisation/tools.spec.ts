import { expect, test } from "@playwright/test";

// Tools: the plan gate when tools.list is absent from the manifest, the catalogue, the grants
// with If-Match on update/retire, the ledger, and the empty/loading/error/gate states.

test.describe.configure({ mode: "serial" });

const TOOL = {
  toolkit: "embedded",
  toolkit_version: 2,
  tool: "repository_search",
  effect: "read",
  idempotency: "idempotent",
  status: "published",
  summary: "Search the tenant's repository",
};
const GRANT = {
  id: "00000000-0000-4000-8000-000000000020",
  job_binding_id: "00000000-0000-4000-8000-000000000021",
  run_id: null,
  toolkit_id: "00000000-0000-4000-8000-000000000022",
  effect_ceiling: "read",
  credential_alias: "search",
  enabled: true,
  version: 3,
};
const CALL = {
  id: "00000000-0000-4000-8000-000000000023",
  run_id: "00000000-0000-4000-8000-000000000024",
  tool: "repository_search",
  toolkit: "embedded",
  toolkit_version: 2,
  status: "ok",
  http_status: 200,
  latency_ms: 42,
};

function json(route: any, body: unknown) {
  return route.fulfill({
    status: 200,
    contentType: "application/json",
    body: JSON.stringify({ output: body }),
  });
}

async function doubleLists(page: any) {
  await page.route("**/api/actions/tools.list/invocations", (route: any) =>
    json(route, { items: [TOOL] }),
  );
  await page.route("**/api/actions/toolkits.list/invocations", (route: any) =>
    json(route, { items: [{ toolkit: "embedded", version: 2, status: "published" }] }),
  );
  await page.route("**/api/actions/tool_bindings.list/invocations", (route: any) =>
    json(route, { items: [GRANT] }),
  );
  await page.route("**/api/actions/tool_calls.list/invocations", (route: any) =>
    json(route, { items: [CALL] }),
  );
}

test("without tools.list the screen is the plan gate naming it", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => entry.name !== "tools.list");
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await page.goto("/tools");
  await expect(page.getByRole("heading", { name: "Tools is planned" })).toBeVisible();
  await expect(page.getByText(/tools\.list is not in this role's manifest/)).toBeVisible();
});

test("the catalogue, grants and ledger render", async ({ page }) => {
  await doubleLists(page);
  await page.goto("/tools");
  await expect(page.getByRole("heading", { name: "Tools", level: 1 })).toBeVisible();
  await expect(page.getByTestId("tools-catalogue")).toContainText("repository_search");
  await expect(page.getByTestId("tools-bindings")).toContainText("read");
  await expect(page.getByTestId("tools-ledger")).toContainText("200");
  await expect(page.getByRole("button", { name: "Publish toolkit" })).toBeVisible();
});

test("a grant retire carries the row's version as If-Match", async ({ page }) => {
  await doubleLists(page);
  let sentIfMatch: string | undefined;
  await page.route("**/api/actions/tool_bindings.retire/invocations", async (route) => {
    sentIfMatch = route.request().headers()["if-match"];
    await json(route, {});
  });

  await page.goto("/tools");
  await page.getByTestId("tools-bindings").getByRole("button", { name: "Retire" }).click();
  const dialog = page.getByRole("dialog", { name: "Retire grant" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Submit" }).click();
  await expect.poll(() => sentIfMatch).toBe("3");
  await expect(dialog).toBeHidden({ timeout: 10_000 });
});

test("the empty catalogue is the app's own message", async ({ page }) => {
  await page.route("**/api/actions/tools.list/invocations", (route) => json(route, { items: [] }));
  await page.route("**/api/actions/toolkits.list/invocations", (route) => json(route, { items: [] }));
  await page.route("**/api/actions/tool_bindings.list/invocations", (route) =>
    json(route, { items: [] }),
  );
  await page.route("**/api/actions/tool_calls.list/invocations", (route) =>
    json(route, { items: [] }),
  );
  await page.goto("/tools");
  await expect(
    page.getByText(/No published tool in this tenant; toolkits\.publish imports/),
  ).toBeVisible();
});

test("the loading state is shown while the catalogue read is in flight", async ({ page }) => {
  await page.route("**/api/actions/tools.list/invocations", async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 800));
    await route.continue();
  });
  await page.goto("/tools");
  await expect(page.getByText("Loading…", { exact: true })).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route("**/api/actions/tools.list/invocations", (route) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({
        title: "Internal Server Error",
        status: 500,
        detail: "the tools read failed",
      }),
    }),
  );
  await page.goto("/tools");
  await expect(page.locator(".problem")).toContainText("the tools read failed");
});

test("a signed-out visit is the sign-in gate", async ({ page }) => {
  await page.goto("/tools");
  await page.evaluate(() => {
    document.cookie = "conveyor_dev=; Max-Age=0; Path=/";
  });
  await page.goto("/tools");
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
});
