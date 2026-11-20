import { expect, test } from "./fixture";

import { allowSelfApproval, createBoardCard, invoke } from "./helpers";

// The board is the first screen where a write matters: `cards.move` carries the card's
// version as If-Match, and the lane is the state. Both pointer and keyboard paths are real.

test("keyboard moves a card between lanes and the move survives a reload", async ({
  page,
  request,
}) => {
  const title = `e2e keyboard ${Date.now()}`;
  const fixture = await createBoardCard(request, title);

  await page.goto(`/board/${fixture.boardId}`);
  const card = page.locator(".card", { hasText: title });
  await expect(card).toBeVisible();
  await expect(
    page.locator(".lane", { hasText: "Intake" }).locator(".card", { hasText: title }),
  ).toBeVisible();

  await card.focus();
  await card.press("ArrowRight");
  await expect(
    page.locator(".lane", { hasText: "Active" }).locator(".card", { hasText: title }),
  ).toBeVisible({ timeout: 10_000 });

  // The move is persisted, not just a local re-render.
  await page.reload();
  await expect(
    page.locator(".lane", { hasText: "Active" }).locator(".card", { hasText: title }),
  ).toBeVisible();
});

test("a pointer drag moves a card into another lane", async ({ page, request }) => {
  const title = `e2e drag ${Date.now()}`;
  const fixture = await createBoardCard(request, title);

  await page.goto(`/board/${fixture.boardId}`);
  const card = page.locator(".card", { hasText: title });
  await expect(card).toBeVisible();
  await card.dragTo(page.locator(".lane", { hasText: "Active" }));
  await expect(
    page.locator(".lane", { hasText: "Active" }).locator(".card", { hasText: title }),
  ).toBeVisible({ timeout: 10_000 });
});

test("the New project form offers the tenant's project types", async ({ page, request }) => {
  const catalogue = await invoke<{ items: { code: string; label: string }[] }>(
    request,
    "catalogues.list",
    { kind: "project_type", include_inactive: false },
  );
  const codes = catalogue.items.map((row) => row.code);
  expect(codes.length).toBeGreaterThan(0);

  await page.goto("/boards");
  const select = page.getByLabel("Project type");
  await expect(select).toBeVisible();
  await expect(select.locator("option")).toHaveCount(codes.length);
  const offered = await select
    .locator("option")
    .evaluateAll((options) => options.map((option) => option.value));
  expect(offered).toEqual(codes);
  const selected = await select.inputValue();
  expect(codes).toContain(selected);

  // The default is one the server accepts: create through the form and see the row.
  const title = `Playwright type ${Date.now()}`;
  await page.getByLabel("Project title").fill(title);
  await page.getByRole("button", { name: "Create project" }).click();
  const project = page.locator(".project", { hasText: title });
  const loadMore = page.getByRole("button", { name: "Load more projects" });
  await expect(page.locator(".project").first()).toBeVisible();
  for (let attempt = 0; attempt < 10 && !(await project.isVisible()); attempt += 1) {
    if (!(await loadMore.isVisible())) break;
    await loadMore.click();
    await expect(loadMore).toBeEnabled({ timeout: 10_000 }).catch(() => {});
  }
  await expect(project).toBeVisible();
});

test("one page: the board index reads its projects without per-project detail reads", async ({
  page,
}) => {
  const detailRequests: string[] = [];
  page.on("request", (request) => {
    const path = new URL(request.url()).pathname;
    if (/^\/api\/projects\/[^/]+$/.test(path)) detailRequests.push(path);
  });

  await page.goto("/boards");
  await expect(page.locator(".project").first()).toBeVisible();
  // The boards come with the page; a project with none would still render, so assert one.
  await expect(page.locator(".project .board-link").first()).toBeVisible();
  // Let any trailing detail reads (if the screen still made them) land.
  await page.waitForTimeout(500);
  expect(detailRequests).toEqual([]);
});

test("a project, its card, an edit and an archive round-trip through the UI", async ({
  page,
  request,
}) => {
  const stamp = Date.now();
  const projectTitle = `AAA Playwright board ${stamp}`;
  const cardName = `Playwright card ${stamp}`;

  // #285: archiving is destructive and runs only behind a gate. The dev seed has one principal
  // per tenant, so the tenant allows the requester to decide — the console still requests,
  // decides and invokes; nothing here skips the gate.
  await allowSelfApproval(request, "cards.archive");

  await page.goto("/boards");
  await expect(page.getByRole("heading", { name: "Board", exact: true })).toBeVisible();

  // The project is created by the form under test, not by the helper.
  await page.getByLabel("Project title").fill(projectTitle);
  await page.getByRole("button", { name: "Create project" }).click();

  // The index is a keyset page. This title derives an early key, but a long-lived dev
  // database can still push it past page one, so page until it is on screen.
  const project = page.locator(".project", { hasText: projectTitle });
  const loadMore = page.getByRole("button", { name: "Load more projects" });
  await expect(page.locator(".project").first()).toBeVisible();
  for (let attempt = 0; attempt < 10 && !(await project.isVisible()); attempt += 1) {
    if (!(await loadMore.isVisible())) break;
    await loadMore.click();
    await expect(loadMore).toBeEnabled({ timeout: 10_000 }).catch(() => {});
  }
  await expect(project).toBeVisible();

  await project.getByRole("link", { name: "Board" }).click();

  const intake = page.locator(".lane", { hasText: "Intake" });
  await expect(intake).toBeVisible();
  await intake.getByLabel("Card name").fill(cardName);
  await intake.getByRole("button", { name: "Add card" }).click();

  // The action's effect is proved on the board's own rows before the screen read: under the
  // full suite the screen's refetch can be delayed past a DOM wait (measured in CI 2026-11-20).
  const boardId = page.url().split("/").pop();
  await expect
    .poll(
      async () => {
        const view = await request
          .get(`/api/boards/${boardId}/cards?limit=200`)
          .then((r) => r.json());
        return (view.items ?? []).map((row: { name: string }) => row.name);
      },
      { timeout: 30_000 },
    )
    .toContain(cardName);
  await page.reload();

  const card = page.locator(".card", { hasText: cardName });
  await expect(card).toBeVisible();

  const editedName = `${cardName} edited`;
  await card.getByRole("button", { name: "Edit" }).click();
  await card.getByLabel("Card name").fill(editedName);
  await card.getByLabel("Card description").fill("Edited through the board screen");
  await card.getByRole("button", { name: "Save" }).click();

  // Same proof-then-read as the add: the save's effect on the board's own rows, then the screen.
  await expect
    .poll(
      async () => {
        const view = await request
          .get(`/api/boards/${boardId}/cards?limit=200`)
          .then((r) => r.json());
        return (view.items ?? []).map((row: { name: string }) => row.name);
      },
      { timeout: 30_000 },
    )
    .toContain(editedName);
  await page.reload();

  const edited = page.locator(".card", { hasText: editedName });
  await expect(edited).toBeVisible();
  await expect(edited.locator(".card-title")).toHaveText(editedName);
  await expect(page.getByText(cardName, { exact: true })).toHaveCount(0);

  await edited.getByRole("button", { name: "Archive" }).click();
  await expect(page.locator(".card", { hasText: editedName })).toHaveCount(0);
  await expect(page.locator(".archived")).toContainText(editedName);
});

test("a board beyond 200 cards pages instead of hiding the tail", async ({ page, request }) => {
  const stamp = Date.now();
  const fixture = await createBoardCard(request, `paging-${stamp}`);
  // Page one holds 200 cards; the 202nd is only reachable through the board's card cursor.
  for (let index = 0; index < 200; index += 1) {
    await invoke(request, "cards.create", {
      board_id: fixture.boardId,
      list_id: fixture.active.id,
      name: `bulk ${stamp} ${index}`,
      card_type_code: "bug",
    });
  }
  await invoke(request, "cards.create", {
    board_id: fixture.boardId,
    list_id: fixture.active.id,
    name: `tail card ${stamp}`,
    card_type_code: "bug",
  });

  await page.goto(`/board/${fixture.boardId}`);
  const tail = page.locator(".card", { hasText: `tail card ${stamp}` });
  await expect(page.getByRole("button", { name: "Load more cards" })).toBeVisible();
  await page.getByRole("button", { name: "Load more cards" }).click();
  await expect(tail).toBeVisible({ timeout: 10_000 });
  await expect(page.locator(".problem")).toHaveCount(0);
});

// P2.1: the index is a keyset page too. 51 real projects put the index past its 50-row page,
// and the tail is one cursor away — never silently dropped.
test("more than 50 projects page instead of hiding the tail", async ({ page, request }) => {
  const stamp = Date.now();
  for (let index = 0; index < 51; index += 1) {
    await invoke(request, "projects.create", { title: `e2e paging project ${stamp} ${index}` });
  }

  await page.goto("/boards");
  await expect(page.getByRole("heading", { name: "Board", exact: true })).toBeVisible();
  const projects = page.locator(".project");
  await expect(projects).toHaveCount(50);
  const loadMore = page.getByRole("button", { name: "Load more projects" });
  await expect(loadMore).toBeVisible();
  await loadMore.click();
  await expect.poll(async () => projects.count()).toBeGreaterThan(50);
  await expect(page.locator(".problem")).toHaveCount(0);
});

// P2.2: the authoring verbs the board declares — a label, a lane and an unarchive — each
// round-trip through their actions. The card row has no label pill, so the label's proof is
// the card's own API row; the lane and the unarchive are visible on the screen.
test("a label, a lane and an unarchive round-trip through the UI", async ({ page, request }) => {
  const stamp = Date.now();
  const title = `e2e authoring ${stamp}`;
  const fixture = await createBoardCard(request, title);
  await allowSelfApproval(request, "cards.archive");

  await page.goto(`/board/${fixture.boardId}`);
  // The screen's heading is the board's name, not the project's; the fixture's card is the mark.
  await expect(page.locator(".card", { hasText: title })).toBeVisible();

  // A label: created through the panel's form, then attached to the start card's row.
  const label = `e2e-label-${stamp}`;
  await page.getByLabel("Label name").fill(label);
  await page.getByLabel("Label color").fill("teal");
  await page.getByRole("button", { name: "Create label" }).click();
  await expect(page.locator(".label", { hasText: label })).toBeVisible();

  // The panel's per-card rows, scoped to the labels panel: an unscoped `div` filter reaches the
  // card's own selects first.
  const labelsPanel = page.locator("section.panel", { hasText: "Board labels" });
  const row = labelsPanel
    .locator("div")
    .filter({ hasText: title })
    .filter({ has: page.locator("select") })
    .last();
  await row.locator("select").selectOption({ label });
  await row.getByRole("button", { name: "Add label" }).click();
  await expect
    .poll(async () => {
      const view = await request.get(`/api/cards/${fixture.card.id}`).then((r) => r.json());
      return (view.labels ?? []).map((entry: { name: string }) => entry.name);
    })
    .toContain(label);

  // A lane: appended through the form. The action's effect is proved on the board's own row —
  // under the full suite the burst of start-process jobs from the >50-project case can delay the
  // screen's refetch past a DOM wait — and the screen then renders it from the server on reload.
  const lane = `e2e lane ${stamp}`;
  await page.getByLabel("New lane name").fill(lane);
  await page.getByRole("button", { name: "Create lane" }).click();
  await expect
    .poll(
      async () => {
        const board = await request.get(`/api/boards/${fixture.boardId}`).then((r) => r.json());
        return (board.lists ?? []).map((entry: { list: { name: string } }) => entry.list.name);
      },
      { timeout: 30_000 },
    )
    .toContain(lane);
  await page.reload();
  await expect(page.locator(".lane", { hasText: lane })).toBeVisible();

  // Archive, then unarchive from the panel: the button reads the card's current version first.
  // The card is a `feature`: the seeded delivery matrix gives `bug` no archive→intake edge, so
  // a bug card's unarchive is refused by the type, not by the screen.
  const featureName = `e2e feature ${stamp}`;
  await invoke(request, "cards.create", {
    board_id: fixture.boardId,
    list_id: fixture.intake.id,
    name: featureName,
    card_type_code: "feature",
  });
  await page.reload();
  const card = page.locator(".card", { hasText: featureName });
  await card.getByRole("button", { name: "Archive" }).click();
  await expect(page.locator(".archived")).toContainText(featureName);
  await page.locator(".archived").getByRole("button", { name: "Unarchive" }).click();
  await expect(page.locator(".card", { hasText: featureName })).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
});
