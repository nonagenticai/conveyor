import { expect, test } from "@playwright/test";

// Runs: the queue, the `run` SSE refetch, the detail with its linked card and the gated cancel,
// and the empty/loading/error/gate states. The dev seed has no runs, so the populated states are
// route doubles of the server's RunOut shape.

test.describe.configure({ mode: "serial" });

const RUN = {
  id: "00000000-0000-4000-8000-000000000010",
  card_id: "00000000-0000-4000-8000-000000000011",
  attempt: 1,
  status: "running",
  version: 3,
  error_class: null,
  created_at: "2026-11-27T10:00:00Z",
};

function json(route: any, body: unknown) {
  return route.fulfill({
    status: 200,
    contentType: "application/json",
    body: JSON.stringify({ output: body }),
  });
}

test("the queue renders the run with its card link and status", async ({ page }) => {
  await page.route("**/api/actions/runs.list/invocations", (route) =>
    json(route, { items: [RUN] }),
  );
  await page.goto("/runs");
  await expect(page.getByRole("heading", { name: "Runs", level: 1 })).toBeVisible();
  await expect(page.getByTestId("runs-queue")).toContainText("running");
  await expect(page.getByRole("link", { name: "00000000-0" })).toHaveCount(2);
});

test("an SSE run frame refetches the queue", async ({ page }) => {
  await page.route("**/api/actions/runs.list/invocations", (route) =>
    json(route, { items: [RUN] }),
  );
  await page.route(/\/api\/events\?topics=run/, (route) =>
    route.fulfill({
      status: 200,
      contentType: "text/event-stream",
      body: "id: 7\nevent: run.updated\ndata: {}\n\n",
    }),
  );
  let lists = 0;
  page.on("request", (request) => {
    if (request.url().includes("/api/actions/runs.list/invocations")) lists += 1;
  });
  await page.goto("/runs");
  await expect.poll(() => lists, { timeout: 15_000 }).toBeGreaterThan(1);
});

test("the detail renders the run and the linked card", async ({ page }) => {
  await page.route("**/api/actions/runs.get/invocations", (route) => json(route, RUN));
  await page.goto(`/runs/${RUN.id}`);
  await expect(page.getByRole("heading", { name: /Run 00000000-0/ })).toBeVisible();
  await expect(page.getByRole("link", { name: RUN.card_id })).toBeVisible();
  await expect(page.getByRole("button", { name: "Cancel run" })).toBeVisible();
});

test("a cancel carries the run id, then refetches", async ({ page }) => {
  let gets = 0;
  await page.route("**/api/actions/runs.get/invocations", (route) => {
    gets += 1;
    return json(route, RUN);
  });
  await page.route("**/api/actions/runs.cancel/invocations", (route) =>
    json(route, { id: RUN.id, status: "cancelled" }),
  );

  await page.goto(`/runs/${RUN.id}`);
  await page.getByRole("button", { name: "Cancel run" }).click();
  const dialog = page.getByRole("dialog", { name: "Cancel run" });
  await expect(dialog).toBeVisible();

  const sent = page.waitForRequest((request) =>
    request.url().includes("/api/actions/runs.cancel/invocations"),
  );
  await dialog.getByRole("button", { name: "Submit" }).click();
  const request = await sent;
  expect(request.postDataJSON().input.id).toBe(RUN.id);
  await expect(dialog).toBeHidden({ timeout: 10_000 });
  await expect.poll(() => gets, { timeout: 10_000 }).toBeGreaterThan(1);
});

test("the empty queue is the app's own message", async ({ page }) => {
  await page.route("**/api/actions/runs.list/invocations", (route) => json(route, { items: [] }));
  await page.goto("/runs");
  await expect(page.getByText("No runs in this filter.", { exact: true })).toBeVisible();
});

test("the loading state is shown while the queue read is in flight", async ({ page }) => {
  await page.route("**/api/actions/runs.list/invocations", async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 800));
    await route.continue();
  });
  await page.goto("/runs");
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
        detail: "the run read failed",
      }),
    }),
  );
  await page.goto("/runs");
  await expect(page.locator(".problem")).toContainText("the run read failed");
});

test("a signed-out visit is the sign-in gate", async ({ page }) => {
  await page.goto("/runs");
  await page.evaluate(() => {
    document.cookie = "conveyor_dev=; Max-Age=0; Path=/";
  });
  await page.goto("/runs");
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
});
