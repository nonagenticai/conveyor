import { expect, test } from "@playwright/test";

// The card cockpit (S4): the card, its runs and its approvals with the tenant's policy per gate.
// cards.update carries the card's version as If-Match; approvals.decide is status-guarded and
// carries none; runs.create carries an Idempotency-Key. Plus the empty / loading / error /
// manifest / sign-in states.

test.describe.configure({ mode: "serial" });

const CARD_ID = "00000000-0000-4000-8000-0000000000a0";
const RUN_ID = "00000000-0000-4000-8000-0000000000a1";
const APPROVAL_ID = "00000000-0000-4000-8000-0000000000a2";
const CARD = {
  card: {
    id: CARD_ID,
    board_id: "00000000-0000-4000-8000-0000000000a3",
    list_id: "00000000-0000-4000-8000-0000000000a4",
    card_type_code: "feature",
    name: "S4 cockpit card",
    description: "the cockpit body",
    position: 1,
    status: "ready",
    version: 7,
    assignee_id: null,
  },
  labels: [{ id: "00000000-0000-4000-8000-0000000000a5", name: "urgent" }],
  comments: [],
  task_lists: [],
  custom_field_values: [],
};
const RUNS = [{ id: RUN_ID, card_id: CARD_ID, status: "queued" }];
const APPROVALS = [
  {
    id: APPROVAL_ID,
    card_id: CARD_ID,
    gate: "cards.archive",
    kind: "action",
    status: "pending",
    target: "card:demo",
  },
];

function json(route: any, body: unknown) {
  return route.fulfill({
    status: 200,
    contentType: "application/json",
    body: JSON.stringify({ output: body }),
  });
}

async function stubCard(
  page: any,
  card: unknown = CARD,
  runs: unknown[] = RUNS,
  approvals: unknown[] = APPROVALS,
) {
  await page.route(`**/api/cards/${CARD_ID}`, (route: any) =>
    route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(card) }),
  );
  await page.route("**/api/actions/runs.list/invocations", (route: any) =>
    json(route, { items: runs }),
  );
  await page.route("**/api/actions/approvals.list/invocations", (route: any) =>
    json(route, { items: approvals }),
  );
  await page.route("**/api/actions/approval_policies.get/invocations", (route: any) =>
    json(route, { approval: "required", self_approval: "forbidden" }),
  );
}

test("the cockpit renders the card, its runs and its approvals with the policy", async ({ page }) => {
  await stubCard(page);
  await page.goto(`/card/${CARD_ID}`);
  await expect(page.getByRole("heading", { name: "S4 cockpit card", level: 1 })).toBeVisible();
  await expect(page.getByText(/version 7/)).toBeVisible();
  await expect(page.getByText("the cockpit body")).toBeVisible();
  await expect(page.getByText("urgent")).toBeVisible();
  await expect(page.getByRole("button", { name: "Edit" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Claim" })).toBeVisible();
  await expect(page.getByRole("button", { name: "New run" })).toBeVisible();
  const runs = page.getByTestId("cockpit-runs");
  await expect(runs).toContainText("queued");
  await expect(runs.getByRole("button", { name: "Start" })).toBeVisible();
  const approvals = page.getByTestId("cockpit-approvals");
  await expect(approvals).toContainText("cards.archive");
  await expect(approvals).toContainText("required · self forbidden");
  await expect(approvals.getByRole("button", { name: "Decide" })).toBeVisible();
});

test("a card with no run and no approval says so", async ({ page }) => {
  await stubCard(page, CARD, [], []);
  await page.goto(`/card/${CARD_ID}`);
  await expect(page.getByText("This card has no run yet.", { exact: true })).toBeVisible();
  await expect(page.getByText("This card has no approval waiting.", { exact: true })).toBeVisible();
});

test("the loading state is shown while the card read is in flight", async ({ page }) => {
  await page.route(`**/api/cards/${CARD_ID}`, async (route: any) => {
    await new Promise((resolve) => setTimeout(resolve, 800));
    await route.continue();
  });
  await page.goto(`/card/${CARD_ID}`);
  await expect(page.getByText("Loading…", { exact: true })).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route(`**/api/cards/${CARD_ID}`, (route: any) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({
        title: "Internal Server Error",
        status: 500,
        detail: "the card read failed",
      }),
    }),
  );
  await page.goto(`/card/${CARD_ID}`);
  await expect(page.locator(".problem")).toContainText("the card read failed");
});

test("without cards.update the edit control is absent", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route: any) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => entry.name !== "cards.update");
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await stubCard(page);
  await page.goto(`/card/${CARD_ID}`);
  await expect(page.getByRole("heading", { name: "S4 cockpit card", level: 1 })).toBeVisible();
  await expect(page.getByRole("button", { name: "Edit" })).toHaveCount(0);
});

test("cards.update carries the card's version as If-Match", async ({ page }) => {
  await stubCard(page);
  let sentIfMatch: string | undefined;
  await page.route("**/api/actions/cards.update/invocations", async (route: any) => {
    sentIfMatch = route.request().headers()["if-match"];
    await json(route, {});
  });
  await page.goto(`/card/${CARD_ID}`);
  await page.getByRole("button", { name: "Edit" }).click();
  const dialog = page.getByRole("dialog", { name: "Edit card" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Submit" }).click();
  await expect.poll(() => sentIfMatch).toBe("7");
});

test("runs.create carries an Idempotency-Key", async ({ page }) => {
  await stubCard(page);
  let sentKey: string | undefined;
  await page.route("**/api/actions/runs.create/invocations", async (route: any) => {
    sentKey = route.request().headers()["idempotency-key"];
    await json(route, { id: RUN_ID });
  });
  await page.goto(`/card/${CARD_ID}`);
  await page.getByRole("button", { name: "New run" }).click();
  const dialog = page.getByRole("dialog", { name: "New run" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Submit" }).click();
  await expect.poll(() => sentKey).toBeTruthy();
});

test("approvals.decide is status-guarded and carries no If-Match", async ({ page }) => {
  await stubCard(page);
  let sentIfMatch: string | undefined;
  await page.route("**/api/actions/approvals.decide/invocations", async (route: any) => {
    sentIfMatch = route.request().headers()["if-match"];
    await json(route, {});
  });
  await page.goto(`/card/${CARD_ID}`);
  await page.getByTestId("cockpit-approvals").getByRole("button", { name: "Decide" }).click();
  const dialog = page.getByRole("dialog", { name: "Decide approval" });
  await expect(dialog).toBeVisible();
  await dialog.getByLabel(/^Decision/).fill("approve");
  await dialog.getByRole("button", { name: "Submit" }).click();
  await expect.poll(() => sentIfMatch).toBeUndefined();
});

test("a signed-out visit is the sign-in gate", async ({ page }) => {
  await page.goto(`/card/${CARD_ID}`);
  await page.evaluate(() => {
    document.cookie = "conveyor_dev=; Max-Age=0; Path=/";
  });
  await page.goto(`/card/${CARD_ID}`);
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
});

// P1.1 — `runs.create` carries an Idempotency-Key, and `runs.start` takes the run's own id.
test("a run is created with an idempotency key and a queued run is started by id", async ({ page }) => {
  await stubCard(page);
  let createdKey: string | undefined;
  await page.route("**/api/actions/runs.create/invocations", async (route: any) => {
    createdKey = route.request().headers()["idempotency-key"];
    await json(route, { id: RUN_ID, card_id: CARD_ID, status: "queued" });
  });
  let started: any = null;
  await page.route("**/api/actions/runs.start/invocations", async (route: any) => {
    started = route.request().postDataJSON();
    await json(route, {});
  });
  await page.goto(`/card/${CARD_ID}`);

  await page.getByRole("button", { name: "New run" }).click();
  await page.getByRole("dialog", { name: "New run" }).getByRole("button", { name: "Submit" }).click();
  await expect.poll(() => createdKey).toBeTruthy();

  await page.getByRole("button", { name: "Start" }).first().click();
  await page.getByRole("dialog", { name: "Start run" }).getByRole("button", { name: "Submit" }).click();
  await expect.poll(() => started?.id).toBe(RUN_ID);
});

// P1.1 — a refusal inside the cockpit sheet is the server's own words.
test("a refused run is shown in the sheet, verbatim", async ({ page }) => {
  await stubCard(page);
  await page.route("**/api/actions/runs.create/invocations", (route: any) =>
    route.fulfill({
      status: 409,
      contentType: "application/problem+json",
      body: JSON.stringify({
        title: "Conflict",
        status: 409,
        detail: "the card is already running",
      }),
    }),
  );
  await page.goto(`/card/${CARD_ID}`);
  await page.getByRole("button", { name: "New run" }).click();
  const dialog = page.getByRole("dialog", { name: "New run" });
  await dialog.getByRole("button", { name: "Submit" }).click();
  await expect(dialog.locator(".form-error")).toContainText("the card is already running");
});
