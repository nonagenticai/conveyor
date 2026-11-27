import { expect, test } from "@playwright/test";

// Jobs: the catalogue, the selected position's releases and bindings, the publish refusal (a
// server-shaped 409 — the walk's dev seed has no capability agent, and the server's own 409 is
// proven by apps/web/e2e/jobs.spec.ts), and the empty/loading/error/gate states.

test.describe.configure({ mode: "serial" });

const HASH = "a".repeat(64);
const JOB = {
  id: "00000000-0000-4000-8000-000000000001",
  key: "position_code_review",
  title: "S1 review position",
  family: "engineering",
  status: "active",
};
const RELEASE = { id: "00000000-0000-4000-8000-000000000002", release_hash: HASH, status: "published", version: 1 };
const BINDING = {
  id: "00000000-0000-4000-8000-000000000003",
  release_hash: HASH,
  executor_kind: "internal_agent",
  status: "active",
};

function json(route: any, body: unknown) {
  return route.fulfill({
    status: 200,
    contentType: "application/json",
    body: JSON.stringify({ output: body }),
  });
}

test("the catalogue and the selected position's releases and bindings render", async ({ page }) => {
  await page.route("**/api/actions/job_catalogue.list/invocations", (route) =>
    json(route, [JOB]),
  );
  await page.route("**/api/actions/job_releases.list/invocations", (route) =>
    json(route, [{ ...RELEASE, release_hash: HASH }]),
  );
  await page.route("**/api/actions/job_bindings.list/invocations", (route) =>
    json(route, [BINDING]),
  );
  await page.route("**/api/actions/capability_agents.list/invocations", (route) =>
    json(route, { items: [] }),
  );

  await page.goto("/jobs");
  await expect(page.getByRole("heading", { name: "Jobs", level: 1 })).toBeVisible();
  await expect(page.getByTestId("jobs-catalogue")).toContainText("S1 review position");

  await page.getByTestId("jobs-catalogue").getByRole("button", { name: "Open" }).click();
  await expect(page.getByTestId("jobs-releases")).toContainText(HASH.slice(0, 14));
  await expect(page.getByTestId("jobs-bindings")).toContainText("internal_agent");
  await expect(page.getByRole("button", { name: "Publish release" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Create binding" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Start run" })).toBeVisible();
});

test("a stale-pin refusal is shown verbatim", async ({ page }) => {
  await page.route("**/api/actions/job_catalogue.list/invocations", (route) =>
    json(route, [JOB]),
  );
  await page.route("**/api/actions/job_releases.list/invocations", (route) =>
    json(route, [RELEASE]),
  );
  await page.route("**/api/actions/job_bindings.list/invocations", (route) =>
    json(route, [BINDING]),
  );
  await page.route("**/api/actions/capability_agents.list/invocations", (route) =>
    json(route, { items: [] }),
  );
  await page.route("**/api/actions/job_releases.publish/invocations", (route) =>
    route.fulfill({
      status: 409,
      contentType: "application/problem+json",
      body: JSON.stringify({
        type: "action-conflict",
        title: "Conflict",
        status: 409,
        code: "job_binding_stale",
        detail: "the binding pins another release",
        request_id: "req-9",
      }),
    }),
  );

  await page.goto("/jobs");
  await page.getByTestId("jobs-catalogue").getByRole("button", { name: "Open" }).click();
  await page.getByRole("button", { name: "Publish release" }).click();
  const dialog = page.getByRole("dialog", { name: "Publish release" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Submit" }).click();
  await expect(dialog.locator(".form-error")).toContainText("the binding pins another release");
  await dialog.getByRole("button", { name: "Close" }).click();
});

test("the empty catalogue is the app's own message", async ({ page }) => {
  await page.goto("/jobs");
  await expect(
    page.getByText("No catalogue positions in this tenant yet.", { exact: true }),
  ).toBeVisible();
});

test("the loading state is shown while the catalogue read is in flight", async ({ page }) => {
  await page.route("**/api/actions/job_catalogue.list/invocations", async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 800));
    await route.continue();
  });
  await page.goto("/jobs");
  await expect(page.getByText("Loading…", { exact: true })).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route("**/api/actions/job_catalogue.list/invocations", (route) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({
        title: "Internal Server Error",
        status: 500,
        detail: "the jobs read failed",
      }),
    }),
  );
  await page.goto("/jobs");
  await expect(page.locator(".problem")).toContainText("the jobs read failed");
});

test("a signed-out visit is the sign-in gate", async ({ page }) => {
  await page.goto("/jobs");
  await page.evaluate(() => {
    document.cookie = "conveyor_dev=; Max-Age=0; Path=/";
  });
  await page.goto("/jobs");
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
});
