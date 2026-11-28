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

// #912. The seven write verbs apps/web carries and the walk did not: a lane's name and position,
// a card's labels, its custom-field values, and an archived card. Each is manifest-gated and each
// sends the input keys its action declares.
const LABEL_ID = "00000000-0000-4000-8000-000000000084";
const FIELD_ID = "00000000-0000-4000-8000-000000000085";

async function boardRoutes(page: any, opts: { labels?: any[]; fields?: any[] } = {}) {
  await page.route(`**/api/boards/${BOARD_ID}`, (route: any) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        board: { id: BOARD_ID, name: "S1 board" },
        lists: [{ list: { id: LANE_ID, name: "Queue", kind: "queue" } }],
        labels: opts.labels ?? [],
        fields: opts.fields ?? [],
      }),
    }),
  );
  await page.route(`**/api/boards/${BOARD_ID}/cards?limit=200`, (route: any) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ items: [CARD] }),
    }),
  );
}

test("a lane can be renamed and moved, and each sends the lane id", async ({ page }) => {
  await boardRoutes(page);
  const sent: Record<string, any> = {};
  await page.route("**/api/actions/lists.update/invocations", async (route: any) => {
    sent.update = route.request().postDataJSON();
    await json(route, {});
  });
  await page.route("**/api/actions/lists.move/invocations", async (route: any) => {
    sent.move = route.request().postDataJSON();
    await json(route, {});
  });
  await page.goto(`/board/${BOARD_ID}`);

  await page.getByRole("button", { name: "Rename lane" }).click();
  const rename = page.getByRole("dialog", { name: "Rename lane" });
  await expect(rename.locator('input[name="name"]')).toHaveValue("Queue");
  await rename.getByRole("button", { name: "Submit" }).click();
  await expect.poll(() => sent.update?.id).toBe(LANE_ID);

  await page.getByRole("button", { name: "Move lane" }).click();
  const move = page.getByRole("dialog", { name: "Move lane" });
  await move.getByRole("button", { name: "Submit" }).click();
  await expect.poll(() => sent.move?.id).toBe(LANE_ID);
});

test("a card's label is added and removed with the board's own label", async ({ page }) => {
  await boardRoutes(page, { labels: [{ id: LABEL_ID, name: "bug", colour: "red" }] });
  const sent: Record<string, any> = {};
  for (const verb of ["cards.add_label", "cards.remove_label"]) {
    await page.route(`**/api/actions/${verb}/invocations`, async (route: any) => {
      sent[verb] = route.request().postDataJSON();
      await json(route, {});
    });
  }
  await page.goto(`/board/${BOARD_ID}`);

  await page.getByRole("button", { name: "Add label" }).click();
  await page.getByRole("dialog", { name: "Add label" }).getByRole("button", { name: "Submit" }).click();
  await expect
    .poll(() => [sent["cards.add_label"]?.card_id, sent["cards.add_label"]?.label_id])
    .toEqual([CARD.id, LABEL_ID]);

  await page.getByRole("button", { name: "Remove label" }).click();
  await page
    .getByRole("dialog", { name: "Remove label" })
    .getByRole("button", { name: "Submit" })
    .click();
  await expect
    .poll(() => [sent["cards.remove_label"]?.card_id, sent["cards.remove_label"]?.label_id])
    .toEqual([CARD.id, LABEL_ID]);
});

test("a field value is set, and clearing it is destructive", async ({ page }) => {
  await boardRoutes(page, { fields: [{ id: FIELD_ID, name: "points", kind: "number" }] });
  let set: any = null;
  await page.route("**/api/actions/custom_fields.set_value/invocations", async (route: any) => {
    set = route.request().postDataJSON();
    await json(route, {});
  });
  await page.goto(`/board/${BOARD_ID}`);

  await page.getByRole("button", { name: "Set value" }).click();
  await page
    .getByRole("dialog", { name: "Set field value" })
    .getByRole("button", { name: "Submit" })
    .click();
  await expect.poll(() => [set?.card_id, set?.custom_field_id]).toEqual([CARD.id, FIELD_ID]);

  // `custom_fields.clear_value` is effect = "destructive": the sheet demands an approval id.
  await page.getByRole("button", { name: "Clear value" }).click();
  const clear = page.getByRole("dialog", { name: "Clear field value" });
  await clear.getByRole("button", { name: "Submit with approval" }).click();
  await expect(clear.locator(".form-error")).toContainText(
    "an approval id is required for a destructive action",
  );
});

test("a card archived in this session is offered for unarchive", async ({ page }) => {
  await boardRoutes(page);
  await page.route("**/api/actions/cards.archive/invocations", (route: any) => json(route, {}));
  let unarchived: any = null;
  await page.route("**/api/actions/cards.unarchive/invocations", async (route: any) => {
    unarchived = route.request().postDataJSON();
    await json(route, {});
  });
  await page.goto(`/board/${BOARD_ID}`);

  await page.getByRole("button", { name: "Archive" }).click();
  const archive = page.getByRole("dialog", { name: "Archive card" });
  await archive.getByLabel("Approval id").fill("00000000-0000-4000-8000-0000000000aa");
  await archive.getByRole("button", { name: "Submit with approval" }).click();

  // The server has no archived-card read, so the list is this session's own — as apps/web keeps it.
  await expect(page.getByRole("heading", { name: "Archived this session" })).toBeVisible();
  await page.getByRole("button", { name: "Unarchive" }).click();
  await page
    .getByRole("dialog", { name: "Unarchive card" })
    .getByRole("button", { name: "Submit" })
    .click();
  await expect.poll(() => unarchived?.id).toBe(CARD.id);
});

test("with the verbs absent from the manifest, none of the new controls render", async ({ page }) => {
  await boardRoutes(page, { labels: [{ id: LABEL_ID, name: "bug" }], fields: [{ id: FIELD_ID, name: "points" }] });
  await page.route(/\/api\/actions$/, async (route: any) => {
    const response = await route.fetch();
    const body = await response.json();
    const hidden = new Set([
      "lists.update",
      "lists.move",
      "cards.add_label",
      "cards.remove_label",
      "custom_fields.set_value",
      "custom_fields.clear_value",
      "cards.unarchive",
    ]);
    body.actions = (body.actions ?? []).filter((entry: any) => !hidden.has(entry.name));
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await page.goto(`/board/${BOARD_ID}`);
  await expect(page.getByRole("heading", { name: "Queue" })).toBeVisible();
  for (const name of ["Rename lane", "Move lane", "Add label", "Remove label", "Set value", "Clear value"]) {
    await expect(page.getByRole("button", { name })).toHaveCount(0);
  }
});
