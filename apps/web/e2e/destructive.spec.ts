import { expect, test, type APIRequestContext } from "@playwright/test";

import { createBoardCard, invoke } from "./helpers";

// #285 — the console's destructive path. A destructive action answers `403 approval_required`
// unless the call names an approved approval whose target equals the target the action
// computes (#282). These specs drive the console through that gate: it asks, the person is told
// what is waiting, and the approvals screen is where the target and the input are read.

/** The tenant's decision for one action, so each spec states the policy it is testing. */
async function setPolicy(
  request: APIRequestContext,
  action: string,
  approval: "required" | "waived",
  selfApproval: "forbidden" | "allowed",
): Promise<void> {
  await invoke(request, "approval_policies.set", {
    action,
    approval,
    self_approval: selfApproval,
  });
}

type Gate = { id: string; gate: string; status: string; target?: string };

/** The tenant's pending gates, newest first. */
async function pendingGates(request: APIRequestContext): Promise<Gate[]> {
  const page = await invoke<{ items: Gate[] }>(request, "approvals.list", {
    status: "pending",
    limit: 50,
  });
  return page.items;
}

/** `cards.archive`'s target for one card — the string the invoke boundary compares. */
function archiveTarget(cardId: string): string {
  return `card/${cardId}/archive`;
}

/** A board, a card, and the gate its archive asked for, created through the console. */
async function waitForArchiveGate(
  page: import("@playwright/test").Page,
  request: APIRequestContext,
  title: string,
): Promise<{ cardId: string; target: string }> {
  const fixture = await createBoardCard(request, title);
  const target = archiveTarget(fixture.card.id);
  await page.goto(`/board/${fixture.boardId}`);
  const card = page.locator(".card", { hasText: title });
  await expect(card).toBeVisible();
  await card.getByRole("button", { name: "Archive" }).click();
  // The console says what is waiting; it does not answer a bare 403 in silence.
  await expect(page.locator(".toast")).toContainText("Waiting for approval");
  return { cardId: fixture.card.id, target };
}

test("a gated delete asks before it deletes", async ({ page, request }) => {
  await setPolicy(request, "cards.archive", "required", "forbidden");
  const title = `e2e gated archive ${Date.now()}`;

  const fixture = await createBoardCard(request, title);
  await page.goto(`/board/${fixture.boardId}`);
  const card = page.locator(".card", { hasText: title });
  await expect(card).toBeVisible();

  await card.getByRole("button", { name: "Archive" }).click();
  await expect(page.locator(".toast")).toContainText("Waiting for approval");

  // Nothing was destroyed: the card is still on the board.
  await page.reload();
  await expect(page.locator(".card", { hasText: title })).toBeVisible();
  await expect(page.locator(".archived")).not.toContainText(title);

  // The gate exists, it names the action, and it carries the computed target.
  const gate = (await pendingGates(request)).find(
    (row) => row.target === archiveTarget(fixture.card.id),
  );
  expect(gate, "the console's archive left a pending gate naming its target").toBeTruthy();
  expect(gate?.gate).toBe("cards.archive");
});

test("a waived action runs without a gate", async ({ page, request }) => {
  await setPolicy(request, "cards.archive", "waived", "forbidden");
  const title = `e2e waived archive ${Date.now()}`;

  const fixture = await createBoardCard(request, title);
  await page.goto(`/board/${fixture.boardId}`);
  const card = page.locator(".card", { hasText: title });
  await expect(card).toBeVisible();
  await card.getByRole("button", { name: "Archive" }).click();

  // The card moves to the archive strip; there is no "waiting" toast.
  await expect(page.locator(".archived")).toContainText(title, { timeout: 15_000 });
  await expect(page.locator(".toast")).toHaveCount(0);

  // No gate was opened for it.
  const gates = await pendingGates(request);
  expect(gates.some((row) => row.target === archiveTarget(fixture.card.id))).toBe(false);
});

test("a forbidden action is not offered", async ({ page, request }) => {
  const title = `e2e forbidden archive ${Date.now()}`;
  const fixture = await createBoardCard(request, title);

  // The registry is the UI's contract: a role's manifest omits what it cannot invoke, and the
  // control follows the manifest — the approval gate is not the authorisation model. Serving the
  // same manifest without `cards.archive` is the deterministic way to hold that role in place.
  await page.route("**/api/actions", async (route) => {
    const response = await route.fetch();
    const body = (await response.json()) as { actions?: { name: string }[] };
    const actions = (body.actions ?? []).filter((action) => action.name !== "cards.archive");
    await route.fulfill({ response, json: { ...body, actions } });
  });

  await page.goto(`/board/${fixture.boardId}`);
  const card = page.locator(".card", { hasText: title });
  await expect(card).toBeVisible();
  await expect(card.getByRole("button", { name: "Archive" })).toHaveCount(0);
});

test("the approvals screen shows the target and the input", async ({ page, request }) => {
  await setPolicy(request, "cards.archive", "required", "forbidden");
  const title = `e2e approvals screen ${Date.now()}`;
  const { cardId, target } = await waitForArchiveGate(page, request, title);

  await page.goto("/approvals");
  const row = page.locator(`.approval-row[data-target="${target}"]`);
  await expect(row).toBeVisible();
  await expect(row.locator(".action")).toHaveText("cards.archive");
  await expect(row.locator(".approval-target")).toHaveText(target);
  // The proposal is the input the approved call will run with, not a summary of it.
  await expect(row.locator(".proposed")).toContainText(cardId);
  await expect(row).toContainText("Alice");
});

test("a self gate is shown but not decidable", async ({ page, request }) => {
  await setPolicy(request, "cards.archive", "required", "forbidden");
  const title = `e2e self gate ${Date.now()}`;
  const { target } = await waitForArchiveGate(page, request, title);

  await page.goto("/approvals");
  const row = page.locator(`.approval-row[data-target="${target}"]`);
  await expect(row).toBeVisible();

  // Shown, with the reason — not hidden, which would read as "nothing to do here".
  await expect(row.getByRole("button", { name: "Approve" })).toBeDisabled();
  await expect(row.getByRole("button", { name: "Reject" })).toBeDisabled();
  await expect(row.locator(".refusal")).toContainText("another person must decide");
});
