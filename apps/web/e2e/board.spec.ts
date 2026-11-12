import { expect, test } from "@playwright/test";

import { createBoardCard, invoke } from "./helpers";

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

test("a project, its card, an edit and an archive round-trip through the UI", async ({
  page,
}) => {
  const stamp = Date.now();
  const projectTitle = `AAA Playwright board ${stamp}`;
  const cardName = `Playwright card ${stamp}`;

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

  const card = page.locator(".card", { hasText: cardName });
  await expect(card).toBeVisible();

  const editedName = `${cardName} edited`;
  await card.getByRole("button", { name: "Edit" }).click();
  await card.getByLabel("Card name").fill(editedName);
  await card.getByLabel("Card description").fill("Edited through the board screen");
  await card.getByRole("button", { name: "Save" }).click();

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
