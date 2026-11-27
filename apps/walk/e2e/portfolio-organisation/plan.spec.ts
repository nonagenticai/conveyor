import { expect, test } from "@playwright/test";

// Plan and the board: the project's WBS/baselines/registers, the real change_rationale_required
// refusal, the board's lanes and cards, an If-Match move, the destructive archive gate, and the
// empty/loading/error/gate states.

test.describe.configure({ mode: "serial" });

const PROJECT = {
  id: "00000000-0000-4000-8000-000000000080",
  key: "S1-PLAN",
  name: "S1 plan project",
  status: "active",
};
const BOARD_ID = "00000000-0000-4000-8000-000000000081";
const LANE_ID = "00000000-0000-4000-8000-000000000082";
const CARD = {
  id: "00000000-0000-4000-8000-000000000083",
  name: "S1 board card",
  list_id: LANE_ID,
  card_type: "feature",
  status: "ready",
  version: 5,
};

function json(route: any, body: unknown) {
  return route.fulfill({
    status: 200,
    contentType: "application/json",
    body: JSON.stringify({ output: body }),
  });
}

test("the plan lists the projects and the selected project's plan", async ({ page }) => {
  await page.route(/\/api\/projects\?limit=200$/, (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ items: [PROJECT] }),
    }),
  );
  await page.route("**/api/actions/wbs.list/invocations", (route) =>
    json(route, { items: [{ title: "S1 WBS item", kind: "deliverable", status: "open", version: 1 }] }),
  );
  await page.route("**/api/actions/baselines.list/invocations", (route) =>
    json(route, { items: [{ name: "S1 baseline", status: "taken", version: 1 }] }),
  );
  await page.route("**/api/actions/registers.list/invocations", (route) =>
    json(route, { items: [{ title: "S1 risk", kind: "risk", status: "open" }] }),
  );

  await page.goto("/plan");
  await expect(page.getByRole("heading", { name: "Plan", level: 1 })).toBeVisible();
  await page.getByTestId("plan-projects").getByRole("button", { name: "Open" }).click();
  await expect(page.getByTestId("plan-wbs")).toContainText("S1 WBS item");
  await expect(page.getByTestId("plan-baselines")).toContainText("S1 baseline");
  await expect(page.getByTestId("plan-registers")).toContainText("S1 risk");
  await expect(page.getByRole("button", { name: "Request change" })).toBeVisible();
});

test("a change without a rationale is the server's change_rationale_required", async ({ page }) => {
  await page.route(/\/api\/projects\?limit=200$/, (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ items: [PROJECT] }),
    }),
  );
  await page.route("**/api/actions/wbs.list/invocations", (route) => json(route, { items: [] }));
  await page.route("**/api/actions/baselines.list/invocations", (route) => json(route, { items: [] }));
  await page.route("**/api/actions/registers.list/invocations", (route) => json(route, { items: [] }));

  await page.goto("/plan");
  await page.getByTestId("plan-projects").getByRole("button", { name: "Open" }).click();
  await page.getByRole("button", { name: "Request change" }).click();
  const dialog = page.getByRole("dialog", { name: "Request change" });
  await expect(dialog).toBeVisible();

  await dialog.getByLabel(/^Job binding id/).fill("00000000-0000-4000-8000-000000000084");
  await dialog.getByLabel(/^Baseline id/).fill("00000000-0000-4000-8000-000000000085");
  await dialog.getByLabel(/^Title/).fill("S1 change");
  await dialog.getByLabel(/^Impact/).fill("3");
  await dialog.getByRole("button", { name: "Submit" }).click();

  await expect(dialog.locator(".form-error")).toContainText("change_rationale_required");
  await dialog.getByRole("button", { name: "Close" }).click();
});

test("the board renders its lanes and cards, and a move carries If-Match", async ({ page }) => {
  await page.route(`**/api/boards/${BOARD_ID}`, (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        board: { id: BOARD_ID, name: "S1 board" },
        lists: [{ list: { id: LANE_ID, name: "Queue", kind: "queue" } }],
        labels: [],
        fields: [],
      }),
    }),
  );
  await page.route(`**/api/boards/${BOARD_ID}/cards?limit=200`, (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ items: [CARD] }),
    }),
  );
  let sentIfMatch: string | undefined;
  await page.route("**/api/actions/cards.move/invocations", async (route) => {
    sentIfMatch = route.request().headers()["if-match"];
    await json(route, {});
  });

  await page.goto(`/board/${BOARD_ID}`);
  await expect(page.getByRole("heading", { name: "S1 board" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Queue" })).toBeVisible();
  await expect(page.getByText("S1 board card")).toBeVisible();

  await page.getByRole("button", { name: "Move" }).click();
  const dialog = page.getByRole("dialog", { name: "Move card" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Submit" }).click();
  await expect.poll(() => sentIfMatch).toBe("5");
});

test("archive is destructive and requires an approval id", async ({ page }) => {
  await page.route(`**/api/boards/${BOARD_ID}`, (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        board: { id: BOARD_ID, name: "S1 board" },
        lists: [{ list: { id: LANE_ID, name: "Queue", kind: "queue" } }],
        labels: [],
        fields: [],
      }),
    }),
  );
  await page.route(`**/api/boards/${BOARD_ID}/cards?limit=200`, (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ items: [CARD] }),
    }),
  );
  await page.goto(`/board/${BOARD_ID}`);
  await page.getByRole("button", { name: "Archive" }).click();
  const dialog = page.getByRole("dialog", { name: "Archive card" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Submit with approval" }).click();
  await expect(dialog.locator(".form-error")).toContainText(
    "an approval id is required for a destructive action",
  );
  await dialog.getByRole("button", { name: "Close" }).click();
});

test("the empty project list is the app's own message", async ({ page }) => {
  await page.route(/\/api\/projects\?limit=200$/, (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ items: [] }),
    }),
  );
  await page.goto("/plan");
  await expect(page.getByText("No project in this tenant yet.", { exact: true })).toBeVisible();
});

test("the loading state is shown while the projects read is in flight", async ({ page }) => {
  await page.route(/\/api\/projects\?limit=200$/, async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 800));
    await route.continue();
  });
  await page.goto("/plan");
  await expect(page.getByText("Loading…", { exact: true })).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route(/\/api\/projects\?limit=200$/, (route) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({
        title: "Internal Server Error",
        status: 500,
        detail: "the plan read failed",
      }),
    }),
  );
  await page.goto("/plan");
  await expect(page.locator(".problem")).toContainText("the plan read failed");
});

test("a signed-out visit is the sign-in gate", async ({ page }) => {
  await page.goto("/plan");
  await page.evaluate(() => {
    document.cookie = "conveyor_dev=; Max-Age=0; Path=/";
  });
  await page.goto("/plan");
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
});
