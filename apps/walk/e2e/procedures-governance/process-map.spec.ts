import { expect, test } from "@playwright/test";

// Process map: the plan gate, the map list, the detail with its ordered steps and the named
// run-position gap, a draft edit carrying If-Match, the empty state and a read failure.

test.describe.configure({ mode: "serial" });

const MAP = {
  id: "00000000-0000-4000-8000-000000000301",
  key: "intake-to-clear",
  title: "Intake to clear map",
  status: "draft",
  version: 2,
  lock_version: 4,
  project_type_id: "00000000-0000-4000-8000-000000000302",
  definition: {
    steps: [
      { key: "intake", title: "Intake the request", kind: "task" },
      { key: "clear", title: "Clear the decision", kind: "gate" },
    ],
  },
};

function json(route: any, body: unknown) {
  return route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ output: body }) });
}

test("without process_maps.list the screen is the plan gate naming it", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => entry.name !== "process_maps.list");
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await page.goto("/build/process-map");
  await expect(page.getByRole("heading", { name: "Process map is planned" })).toBeVisible();
  await expect(page.getByText(/process_maps\.list is not in this role's manifest/)).toBeVisible();
});

test("the list renders each map and its step count", async ({ page }) => {
  await page.route("**/api/actions/process_maps.list/invocations", (route: any) => json(route, { items: [MAP] }));
  await page.goto("/build/process-map");
  await expect(page.getByRole("heading", { name: "Process map", level: 1 })).toBeVisible();
  await expect(page.getByTestId("process-map-list")).toContainText("Intake to clear map");
  await expect(page.getByTestId("process-map-list")).toContainText("2");
  await expect(page.getByRole("button", { name: "New map" })).toBeVisible();
});

test("the detail renders the ordered steps and names the run-position gap", async ({ page }) => {
  await page.route("**/api/actions/process_maps.get/invocations", (route: any) => json(route, MAP));
  await page.goto(`/build/process-map/${MAP.id}`);
  await expect(page.getByRole("heading", { name: "Intake to clear map" })).toBeVisible();
  await expect(page.getByTestId("process-map-steps")).toContainText("Intake the request");
  await expect(page.getByTestId("process-map-steps")).toContainText("gate");
  await expect(page.getByText(/Run positions are not overlaid/)).toBeVisible();
});

test("a draft edit carries the row's lock version as If-Match", async ({ page }) => {
  await page.route("**/api/actions/process_maps.get/invocations", (route: any) => json(route, MAP));
  let sentIfMatch: string | undefined;
  await page.route("**/api/actions/process_maps.update/invocations", async (route: any) => {
    sentIfMatch = route.request().headers()["if-match"];
    await json(route, {});
  });
  await page.goto(`/build/process-map/${MAP.id}`);
  await page.getByRole("button", { name: "Edit" }).click();
  const dialog = page.getByRole("dialog", { name: "Edit Intake to clear map" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Submit" }).click();
  await expect.poll(() => sentIfMatch).toBe("4");
  await expect(dialog).toBeHidden({ timeout: 10_000 });
});

test("the empty list is the app's own message", async ({ page }) => {
  await page.route("**/api/actions/process_maps.list/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/build/process-map");
  await expect(page.getByText("No process map yet. This is the empty state, not an error.")).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route("**/api/actions/process_maps.list/invocations", (route: any) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({ title: "Internal Server Error", status: 500, detail: "the maps read failed" }),
    }),
  );
  await page.goto("/build/process-map");
  await expect(page.locator(".problem")).toContainText("the maps read failed");
});
