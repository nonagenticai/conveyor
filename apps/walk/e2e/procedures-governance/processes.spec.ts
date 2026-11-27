import { expect, test } from "@playwright/test";

// Processes: the plan gate, the list, the detail with its outputs, a draft edit carrying If-Match,
// the empty state, a read failure and the signed-out gate.

test.describe.configure({ mode: "serial" });

const PROCESS = {
  id: "00000000-0000-4000-8000-000000000201",
  key: "intake-to-clear",
  title: "Intake to clear",
  description: "From a submitted intake to a cleared decision.",
  status: "draft",
  version: 4,
  lock_version: 5,
  card_type_code: "decision",
  owner_user_id: "00000000-0000-4000-8000-000000000202",
  outputs: { decision: "cleared" },
};

function json(route: any, body: unknown) {
  return route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ output: body }) });
}

test("without processes.list the screen is the plan gate naming it", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => entry.name !== "processes.list");
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await page.goto("/build/processes");
  await expect(page.getByRole("heading", { name: "Processes is planned" })).toBeVisible();
  await expect(page.getByText(/processes\.list is not in this role's manifest/)).toBeVisible();
});

test("the list renders each process and its state", async ({ page }) => {
  await page.route("**/api/actions/processes.list/invocations", (route: any) => json(route, { items: [PROCESS] }));
  await page.goto("/build/processes");
  await expect(page.getByRole("heading", { name: "Processes", level: 1 })).toBeVisible();
  await expect(page.getByTestId("processes-list")).toContainText("Intake to clear");
  await expect(page.getByTestId("processes-list")).toContainText("draft");
  await expect(page.getByRole("button", { name: "New process" })).toBeVisible();
});

test("the detail renders the process and its outputs", async ({ page }) => {
  await page.route("**/api/actions/processes.get/invocations", (route: any) => json(route, PROCESS));
  await page.route("**/api/actions/process_runs.start/invocations", (route: any) => json(route, {}));
  await page.goto(`/build/processes/${PROCESS.id}`);
  await expect(page.getByRole("heading", { name: "Intake to clear" })).toBeVisible();
  await expect(page.locator("pre.code")).toContainText("cleared");
  await expect(page.getByRole("button", { name: "Start a run" })).toBeVisible();
});

test("a draft edit carries the row's lock version as If-Match", async ({ page }) => {
  await page.route("**/api/actions/processes.get/invocations", (route: any) => json(route, PROCESS));
  let sentIfMatch: string | undefined;
  await page.route("**/api/actions/processes.update/invocations", async (route: any) => {
    sentIfMatch = route.request().headers()["if-match"];
    await json(route, {});
  });
  await page.goto(`/build/processes/${PROCESS.id}`);
  await page.getByRole("button", { name: "Edit" }).click();
  const dialog = page.getByRole("dialog", { name: "Edit Intake to clear" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Submit" }).click();
  await expect.poll(() => sentIfMatch).toBe("5");
  await expect(dialog).toBeHidden({ timeout: 10_000 });
});

test("the empty list is the app's own message", async ({ page }) => {
  await page.route("**/api/actions/processes.list/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/build/processes");
  await expect(page.getByText("No process yet. This is the empty state, not an error.")).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route("**/api/actions/processes.list/invocations", (route: any) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({ title: "Internal Server Error", status: 500, detail: "the processes read failed" }),
    }),
  );
  await page.goto("/build/processes");
  await expect(page.locator(".problem")).toContainText("the processes read failed");
});
