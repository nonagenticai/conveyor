import { expect, test } from "@playwright/test";

// The board index (S4): the app's one projects read carrying each project's boards, the
// project-type catalogue beside them, projects.create's Idempotency-Key, the manifest gate, and
// the empty / loading / error / sign-in states. Read-only except the single create.

test.describe.configure({ mode: "serial" });

const PROJECT_ID = "00000000-0000-4000-8000-000000000090";
const BOARD_ID = "00000000-0000-4000-8000-000000000091";
const PROJECT = {
  project: { id: PROJECT_ID, key: "S4-BOARD", name: "S4 board project", status: "active" },
  boards: [{ id: BOARD_ID, project_id: PROJECT_ID, name: "Delivery board", version: 2 }],
};
const TYPES = [{ code: "delivery", label: "Delivery" }];

function json(route: any, body: unknown) {
  return route.fulfill({
    status: 200,
    contentType: "application/json",
    body: JSON.stringify({ output: body }),
  });
}

async function stubIndex(page: any, items: unknown[] = [PROJECT]) {
  await page.route(/\/api\/projects\?limit=50&include=boards$/, (route: any) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ items, next_cursor: null }),
    }),
  );
  await page.route("**/api/actions/catalogues.list/invocations", (route: any) =>
    json(route, { items: TYPES }),
  );
}

test("the index renders each project with its boards and the project types", async ({ page }) => {
  await stubIndex(page);
  await page.goto("/boards");
  await expect(page.getByRole("heading", { name: "Boards", level: 1 })).toBeVisible();
  const table = page.getByTestId("boards-projects");
  await expect(table).toContainText("S4 board project");
  await expect(table).toContainText("S4-BOARD");
  await expect(table.getByRole("link", { name: "Delivery board" })).toHaveAttribute(
    "href",
    `/board/${BOARD_ID}`,
  );
  await expect(page.getByRole("button", { name: "New project" })).toBeVisible();
  await expect(page.getByText(/delivery — Delivery/)).toBeVisible();
});

test("the empty project list is the app's own message", async ({ page }) => {
  await stubIndex(page, []);
  await page.goto("/boards");
  await expect(page.getByText("No project in this tenant yet.", { exact: true })).toBeVisible();
});

test("the loading state is shown while the projects read is in flight", async ({ page }) => {
  await page.route(/\/api\/projects\?limit=50&include=boards$/, async (route: any) => {
    await new Promise((resolve) => setTimeout(resolve, 800));
    await route.continue();
  });
  await page.goto("/boards");
  await expect(page.getByText("Loading…", { exact: true })).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route(/\/api\/projects\?limit=50&include=boards$/, (route: any) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({
        title: "Internal Server Error",
        status: 500,
        detail: "the board index read failed",
      }),
    }),
  );
  await page.goto("/boards");
  await expect(page.locator(".problem")).toContainText("the board index read failed");
});

test("without projects.create the control is absent", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route: any) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter(
      (entry: any) => entry.name !== "projects.create",
    );
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await stubIndex(page);
  await page.goto("/boards");
  await expect(page.getByRole("heading", { name: "Boards", level: 1 })).toBeVisible();
  await expect(page.getByRole("button", { name: "New project" })).toHaveCount(0);
});

test("projects.create is a schema sheet carrying an Idempotency-Key", async ({ page }) => {
  await stubIndex(page);
  let sentKey: string | undefined;
  await page.route("**/api/actions/projects.create/invocations", async (route: any) => {
    sentKey = route.request().headers()["idempotency-key"];
    await json(route, { id: "00000000-0000-4000-8000-000000000092", key: "S4-NEW" });
  });
  await page.goto("/boards");
  await page.getByRole("button", { name: "New project" }).click();
  const dialog = page.getByRole("dialog", { name: "New project" });
  await expect(dialog).toBeVisible();
  await dialog.getByLabel(/^Title/).fill("S4 new project");
  await dialog.getByRole("button", { name: "Submit" }).click();
  await expect.poll(() => sentKey).toBeTruthy();
});

test("a signed-out visit is the sign-in gate", async ({ page }) => {
  await page.goto("/boards");
  await page.evaluate(() => {
    document.cookie = "conveyor_dev=; Max-Age=0; Path=/";
  });
  await page.goto("/boards");
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
});

// P1.1 — a refusal is the server's, shown verbatim in the sheet (SPEC §5).
test("a refused create is the server's own words", async ({ page }) => {
  await page.route("**/api/actions/projects.create/invocations", (route: any) =>
    route.fulfill({
      status: 422,
      contentType: "application/problem+json",
      body: JSON.stringify({
        title: "Unprocessable Entity",
        status: 422,
        detail: "project_type_code must name a live type",
      }),
    }),
  );
  await page.goto("/boards");
  await page.getByRole("button", { name: "New project" }).click();
  const dialog = page.getByRole("dialog", { name: "New project" });
  await dialog.getByLabel(/^Title/).fill("S4 new project");
  await dialog.getByRole("button", { name: "Submit" }).click();
  await expect(dialog.locator(".form-error")).toContainText(
    "project_type_code must name a live type",
  );
});
