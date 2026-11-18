import { expect, test } from "@playwright/test";

import {
  allowSelfApproval,
  createBoardCard,
  decideAsReviewer,
  invoke,
  isDevAuth,
} from "./helpers";

const SELF_GATE_REASON = "You requested this gate; another person must decide it.";

test("the cockpit claims, starts a run and approves a gate", async ({ page, request }) => {
  const title = `e2e cockpit ${Date.now()}`;
  const fixture = await createBoardCard(request, title);
  const gate = await invoke<{ id: string }>(request, "approvals.request", {
    gate: "e2e gate",
    kind: "human_approval",
    card_id: fixture.card.id,
    description: "created by the cockpit spec",
    required_decisions: 1,
  });

  await page.goto(`/card/${fixture.card.id}`);
  await expect(page.getByRole("heading", { name: title })).toBeVisible();
  await expect(page.getByText("The card")).toBeVisible();
  await expect(page.getByText("e2e gate")).toBeVisible();

  // claim carries If-Match; the action bar flips to Release only after the write succeeded
  await page.getByRole("button", { name: "Claim" }).click();
  await expect(page.getByRole("button", { name: "Release" })).toBeVisible({ timeout: 10_000 });

  // Edit the card under If-Match: the heading is the refetched name, the body the new text.
  const edited = `${title} (edited)`;
  await page.getByRole("button", { name: "Edit" }).click();
  await page.getByLabel("Title").fill(edited);
  await page.getByLabel("Description").fill("edited by the cockpit spec");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByRole("heading", { name: edited })).toBeVisible({ timeout: 10_000 });
  await expect(page.getByText("edited by the cockpit spec")).toBeVisible();

  // create + start a run; the Runs panel gains a row
  await page.getByRole("button", { name: "New run" }).click();
  await expect(
    page.locator(".cockpit section.panel", { hasText: "Runs" }).locator(".row-card"),
  ).not.toHaveCount(0, { timeout: 10_000 });

  // Alice requested this gate, so the console shows it but refuses her the decision and says why.
  await expect(page.getByRole("button", { name: "Approve" })).toBeDisabled();
  await expect(page.getByText(SELF_GATE_REASON)).toBeVisible();

  // Only the dev seed has a second dev-a admin to decide it.
  test.skip(!isDevAuth(), "the deployed suite has one OIDC principal, so nobody else can decide");

  await decideAsReviewer(request, "approvals.decide", gate.id);
  const approved = page.locator(".pill", { hasText: /approved/i });
  // The decision is heard on the cockpit's SSE subscription; a reload is the same read if the
  // frame is missed.
  await expect(approved)
    .toBeVisible({ timeout: 10_000 })
    .catch(() => page.reload());
  await expect(approved).toBeVisible();

  // The decision history names the principal, the actor kind and the channel (the S12 read).
  await expect(page.locator(".pill", { hasText: "Reviewer · human · web" })).toBeVisible();

  await expect(page.locator(".problem")).toHaveCount(0);
});

test("a_self_gate_is_shown_but_not_decidable_in_the_cockpit", async ({ page, request }) => {
  const title = `e2e self gate ${Date.now()}`;
  const fixture = await createBoardCard(request, title);
  await invoke(request, "approvals.request", {
    gate: "e2e self gate",
    kind: "human_approval",
    card_id: fixture.card.id,
    description: "created by the self-gate spec",
    required_decisions: 1,
  });

  await page.goto(`/card/${fixture.card.id}`);
  await expect(page.getByText("e2e self gate", { exact: true })).toBeVisible();

  // The requester's own gate is shown, never hidden — the control is disabled and explains itself.
  await expect(page.getByRole("button", { name: "Approve" })).toBeDisabled();
  await expect(page.getByText(SELF_GATE_REASON)).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("a_self_gate_is_decided_when_the_policy_allows_it", async ({ page, request }) => {
  const title = `e2e self-approved ${Date.now()}`;
  const fixture = await createBoardCard(request, title);
  // The card gate's name is not a registered action, so a policy can never allow it; this one
  // is, and the tenant's policy is what re-enables the requester's own Approve.
  await allowSelfApproval(request, "cards.archive");
  await invoke(request, "approvals.request", {
    gate: "cards.archive",
    kind: "action",
    action: "cards.archive",
    card_id: fixture.card.id,
    input: { id: fixture.card.id },
    proposed_input: { id: fixture.card.id },
    required_decisions: 1,
  });

  await page.goto(`/card/${fixture.card.id}`);
  const approve = page.getByRole("button", { name: "Approve" });
  await expect(approve).toBeEnabled();
  await approve.click();

  // The pill moves off pending: the requester decided their own gate, which the policy allows.
  await expect(page.locator(".pill", { hasText: /approved/i })).toBeVisible({ timeout: 10_000 });
  await expect(page.locator(".pill", { hasText: /^pending$/i })).toHaveCount(0);
  await expect(page.locator(".problem")).toHaveCount(0);
});
