import { expect, test } from "@playwright/test";

// Execution: the tenant's runs, the selected run's steps, and the tool ledger for it, plus the
// empty/loading/error/gate states.

test.describe.configure({ mode: "serial" });

const RUN = {
  id: "00000000-0000-4000-8000-000000000060",
  card_id: "00000000-0000-4000-8000-000000000061",
  status: "running",
  attempt: 1,
  executor_kind: "internal_agent",
  job_binding_id: "00000000-0000-4000-8000-000000000062",
  release_hash: "b".repeat(64),
  started_at: "2026-11-27T10:00:00Z",
  finished_at: null,
  steps: [{ ordinal: 1, action: "repository_search", attempt: 1, status: "succeeded" }],
};
const CALL = {
  tool: "repository_search",
  toolkit_version: 2,
  status: "ok",
  credential_alias: "search",
  evidence: "42",
};

function json(route: any, body: unknown) {
  return route.fulfill({
    status: 200,
    contentType: "application/json",
    body: JSON.stringify({ output: body }),
  });
}

test("the runs render and a selected run shows its steps and tool ledger", async ({ page }) => {
  await page.route("**/api/actions/runs.list/invocations", (route) =>
    json(route, { items: [RUN] }),
  );
  await page.route("**/api/actions/runs.get/invocations", (route) => json(route, RUN));
  await page.route("**/api/actions/tool_calls.list/invocations", (route) =>
    json(route, { items: [CALL] }),
  );
  await page.goto("/execution");
  await expect(page.getByRole("heading", { name: "Execution room", level: 1 })).toBeVisible();
  await expect(page.getByTestId("execution-runs")).toContainText("internal_agent");
  await page.getByTestId("execution-runs").getByRole("button", { name: "00000000" }).click();
  await expect(page.getByTestId("execution-steps")).toContainText("repository_search");
  await expect(page.getByTestId("execution-tool-calls")).toContainText("search");
});

test("the empty run list is the app's own message", async ({ page }) => {
  await page.route("**/api/actions/runs.list/invocations", (route) => json(route, { items: [] }));
  await page.goto("/execution");
  await expect(
    page.getByText("No run in this tenant yet; a board card starts one.", { exact: true }),
  ).toBeVisible();
});

test("the loading state is shown while the run read is in flight", async ({ page }) => {
  await page.route("**/api/actions/runs.list/invocations", async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 800));
    await route.continue();
  });
  await page.goto("/execution");
  await expect(page.getByText("Loading…", { exact: true })).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route("**/api/actions/runs.list/invocations", (route) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({
        title: "Internal Server Error",
        status: 500,
        detail: "the execution read failed",
      }),
    }),
  );
  await page.goto("/execution");
  await expect(page.locator(".problem")).toContainText("the execution read failed");
});

test("a signed-out visit is the sign-in gate", async ({ page }) => {
  await page.goto("/execution");
  await page.evaluate(() => {
    document.cookie = "conveyor_dev=; Max-Age=0; Path=/";
  });
  await page.goto("/execution");
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
});
