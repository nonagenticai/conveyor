import { expect, test, type APIRequestContext, type Page } from "./fixture";

import {
  answerWaiverPrompt,
  archiveTarget,
  createBoardCard,
  decideAsReviewer,
  FIXTURE_EVIDENCE_WAIVER,
  invoke,
  isDevAuth,
  pendingGateFor,
  pendingGates,
  recordGateRequests,
  setApprovalPolicy,
  type PendingGate,
} from "./helpers";

// #285 — the console's destructive path. A destructive action answers `403 approval_required`
// unless the call names an approved approval whose target equals the target the action
// computes (#282). These specs drive the console through that gate: it asks, the person is told
// what is waiting, and the approvals screen is where the target and the input are read.
//
// A gate for a destructive action also requires typed evidence (§5.4), and the console's
// minimal bridge asks the operator to record an explicit waiver reason instead. The fixture
// gates use `FIXTURE_EVIDENCE_WAIVER`; the prompt's cancel / reason / blank paths are asserted
// below.

/**
 * The approvals row for one target.
 *
 * Matched by its text, not by a `data-target` attribute: the row's identity on the screen is
 * what it renders, and a test that leans on an attribute proves the attribute, not the row.
 */
function approvalRow(page: Page, target: string) {
  return page.locator("article").filter({ hasText: target }).first();
}

/** A board, a card, and the gate its archive asked for, created through the console. */
async function waitForArchiveGate(
  page: Page,
  request: APIRequestContext,
  title: string,
): Promise<{ cardId: string; target: string }> {
  const fixture = await createBoardCard(request, title);
  const target = archiveTarget(fixture.card.id);
  // The console's request is refused for want of evidence; the operator records the waiver,
  // and the retry opens the gate.
  answerWaiverPrompt(page, [FIXTURE_EVIDENCE_WAIVER]);
  await page.goto(`/board/${fixture.boardId}`);
  const card = page.locator(".card", { hasText: title });
  await expect(card).toBeVisible();
  await card.getByRole("button", { name: "Archive" }).click();
  // The console says what is waiting; it does not answer a bare 403 in silence.
  await expect(page.locator(".toast")).toContainText("Waiting for approval");
  return { cardId: fixture.card.id, target };
}

test("a gated delete asks before it deletes", async ({ page, request }) => {
  await setApprovalPolicy(request, "cards.archive", "required");
  const title = `e2e gated archive ${Date.now()}`;

  const fixture = await createBoardCard(request, title);
  // The gate request is refused for want of evidence; the operator records the waiver.
  answerWaiverPrompt(page, [FIXTURE_EVIDENCE_WAIVER]);
  await page.goto(`/board/${fixture.boardId}`);
  const card = page.locator(".card", { hasText: title });
  await expect(card).toBeVisible();

  await card.getByRole("button", { name: "Archive" }).click();
  await expect(page.locator(".toast")).toContainText("Waiting for approval");

  // Nothing was destroyed: the card is still on the board, and the archive strip (which only
  // renders when something has been archived in this session) does not carry it.
  await page.reload();
  await expect(page.locator(".card", { hasText: title })).toBeVisible();
  await expect(page.locator(".archived", { hasText: title })).toHaveCount(0);

  // The gate exists, it names the action, and it carries the computed target.
  const gate = (await pendingGates(request)).find(
    (row) => row.target === archiveTarget(fixture.card.id),
  );
  expect(gate, "the console's archive left a pending gate naming its target").toBeTruthy();
  expect(gate?.gate).toBe("cards.archive");
});

test("a waived action runs without a gate", async ({ page, request }) => {
  await setApprovalPolicy(request, "cards.archive", "waived");
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
  await setApprovalPolicy(request, "cards.archive", "required");
  const title = `e2e approvals screen ${Date.now()}`;
  const { cardId, target } = await waitForArchiveGate(page, request, title);

  await page.goto("/approvals");
  const row = approvalRow(page, target);
  await expect(row).toBeVisible();
  await expect(row).toContainText("cards.archive");
  await expect(row).toContainText(target);
  // The proposal is the input the approved call will run with, not a summary of it.
  await expect(row).toContainText(cardId);
  await expect(row).toContainText("Alice");
});

test("a retry after the gate is approved completes the call", async ({ page, request }) => {
  await setApprovalPolicy(request, "cards.archive", "required");
  const title = `e2e approved retry ${Date.now()}`;
  const { target } = await waitForArchiveGate(page, request, title);

  // §5.4: the requester can never decide its own gate, so a distinct principal approves it.
  test.skip(!isDevAuth(), "the deployed suite has one principal, so nobody else can decide");
  const gate = await pendingGateFor(request, target);
  await decideAsReviewer(request, "approvals.decide", gate.id);

  // The requester's retry spends that gate instead of asking for a second one.
  await page
    .locator(".card", { hasText: title })
    .getByRole("button", { name: "Archive" })
    .click();
  await expect(page.locator(".archived")).toContainText(title, { timeout: 15_000 });

  // One gate in total for that object: the retry spent the approved one, it did not ask again.
  const all = await invoke<{ items: PendingGate[] }>(request, "approvals.list", { limit: 200 });
  expect(all.items.filter((row) => row.target === target)).toHaveLength(1);
});

test("a self gate is shown but not decidable", async ({ page, request }) => {
  await setApprovalPolicy(request, "cards.archive", "required");
  const title = `e2e self gate ${Date.now()}`;
  const { target } = await waitForArchiveGate(page, request, title);

  await page.goto("/approvals");
  const row = approvalRow(page, target);
  await expect(row).toBeVisible();

  // Shown, with the reason — not hidden, which would read as "nothing to do here".
  await expect(row.getByRole("button", { name: "Approve" })).toBeDisabled();
  await expect(row.getByRole("button", { name: "Reject" })).toBeDisabled();
  await expect(row).toContainText("another person must decide");
});

// v1-ui-1 — the console's minimal §5.4 bridge. A destructive gate now requires evidence; the
// console asks the operator for an explicit waiver reason on exactly the `422
// gate_evidence_missing` refusal, and retries the same request once with it. The app never
// invents a waiver: Cancel and a blank answer keep the refusal.

test("a cancelled waiver prompt surfaces the refusal and opens no gate", async ({
  page,
  request,
}) => {
  await setApprovalPolicy(request, "cards.archive", "required");
  const title = `e2e cancelled waiver ${Date.now()}`;
  const fixture = await createBoardCard(request, title);
  const target = archiveTarget(fixture.card.id);

  const gates = recordGateRequests(page);
  const prompt = answerWaiverPrompt(page, [null]);

  await page.goto(`/board/${fixture.boardId}`);
  const card = page.locator(".card", { hasText: title });
  await expect(card).toBeVisible();
  await card.getByRole("button", { name: "Archive" }).click();

  // The server's refusal is surfaced unchanged; Cancel retries nothing and sends no waiver.
  await expect(page.locator(".toast")).toContainText("typed evidence reference");
  expect(prompt.dialogs, "the operator is asked exactly once").toBe(1);
  expect(gates, "a cancelled prompt sends no retry").toHaveLength(1);
  expect(gates[0].input.evidence_waiver_reason).toBeUndefined();

  // Nothing was destroyed and no gate was opened.
  await page.reload();
  await expect(page.locator(".card", { hasText: title })).toBeVisible();
  expect((await pendingGates(request)).some((row) => row.target === target)).toBe(false);
});

test("a reason retries the gate request exactly once with the waiver", async ({
  page,
  request,
}) => {
  await setApprovalPolicy(request, "cards.archive", "required");
  const title = `e2e waiver reason ${Date.now()}`;
  const fixture = await createBoardCard(request, title);
  const target = archiveTarget(fixture.card.id);
  const reason = "e2e: this card row is the gate's subject";

  const gates = recordGateRequests(page);
  const prompt = answerWaiverPrompt(page, [reason]);

  await page.goto(`/board/${fixture.boardId}`);
  const card = page.locator(".card", { hasText: title });
  await expect(card).toBeVisible();
  await card.getByRole("button", { name: "Archive" }).click();

  await expect(page.locator(".toast")).toContainText("Waiting for approval");
  expect(prompt.dialogs, "one reason, one prompt").toBe(1);
  // The refusal, then exactly one retry — the same gate request plus the waiver reason.
  expect(gates).toHaveLength(2);
  expect(gates[0].input.evidence_waiver_reason).toBeUndefined();
  expect(gates[1].input.evidence_waiver_reason).toBe(reason);
  expect(gates[1].input).toMatchObject({
    gate: "cards.archive",
    action: "cards.archive",
    input: { id: fixture.card.id },
  });

  // The retry opened the gate the reason is stored on.
  const gate = await pendingGateFor(request, target);
  expect(gate.gate).toBe("cards.archive");
});

test("a blank waiver reason is refused and never sent", async ({ page, request }) => {
  await setApprovalPolicy(request, "cards.archive", "required");
  const title = `e2e blank waiver ${Date.now()}`;
  const fixture = await createBoardCard(request, title);
  const target = archiveTarget(fixture.card.id);

  const gates = recordGateRequests(page);
  // A blank answer is not a waiver: the app asks once more, then keeps the refusal.
  const prompt = answerWaiverPrompt(page, ["", "   "]);

  await page.goto(`/board/${fixture.boardId}`);
  const card = page.locator(".card", { hasText: title });
  await expect(card).toBeVisible();
  await card.getByRole("button", { name: "Archive" }).click();

  await expect(page.locator(".toast")).toContainText("typed evidence reference");
  expect(prompt.dialogs, "a blank answer is asked again, then refused").toBe(2);
  expect(gates, "a blank reason never becomes a waiver retry").toHaveLength(1);
  expect(gates[0].input.evidence_waiver_reason).toBeUndefined();
  expect((await pendingGates(request)).some((row) => row.target === target)).toBe(false);
});
