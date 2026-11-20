import { expect, test } from "@playwright/test";

import { decideAsReviewer, invoke, isDevAuth } from "./helpers";

// Screen 19 — the eval sign-off. A gate is an `approval` row: the requester asks, another person
// decides, and what they sign is the instrument's digest. This spec builds its own eval so it
// neither depends on nor changes the seeded eval's gates (evals.spec.ts decides those).

test("the two gates are requested and decided, and the screen refuses a self-decision", async ({
  page,
  request,
}) => {
  const created = await invoke<{ id: string }>(request, "evals.create", {
    name: `e2e signoff ${Date.now()}`,
    subject_kind: "completions",
    entry_action: "inference.chat",
  });
  await invoke(request, "eval_versions.create", { eval_id: created.id });

  // Empty: every gate is unrequested, and each offers its Request control.
  await page.goto(`/evals/${created.id}/sign-off`);
  const inputs = page.locator("tr", { hasText: "Inputs" });
  const grading = page.locator("tr", { hasText: "Grading" });
  await expect(inputs.locator(".pill")).toHaveText("not requested");
  await expect(grading.locator(".pill")).toHaveText("not requested");
  await expect(inputs.getByRole("button", { name: "Request" })).toBeEnabled();

  // Requesting is the screen's write: the row becomes pending, and the requester cannot decide
  // it — the refusal is the store's rule, stated on the row, not a hidden disabled control.
  await inputs.getByRole("button", { name: "Request" }).click();
  await expect(inputs.locator(".pill")).toHaveText("pending");
  await expect(inputs.getByRole("button", { name: "Approve" })).toBeDisabled();
  await expect(
    inputs.getByText("You requested this gate; another person must decide it."),
  ).toBeVisible();

  // The decision is another principal's — the seed's `dev:reviewer`. The reloaded tab is the
  // read. The deployed suite logs in as one OIDC principal, so it cannot exercise this half.
  test.skip(!isDevAuth(), "the deployed suite has one principal, so nobody else can decide");
  const pending = await invoke<{
    items: { id: string; gate: string; eval_id: string | null }[];
  }>(request, "approvals.list", { status: "pending", limit: 100 });
  const gate = pending.items.find(
    (row) => row.gate === "eval_inputs" && row.eval_id === created.id,
  );
  if (!gate) {
    throw new Error(`the requested inputs gate is not pending: ${JSON.stringify(pending.items)}`);
  }
  await decideAsReviewer(request, "eval_signoffs.decide", gate.id);

  await page.reload();
  await expect(inputs.locator(".pill")).toHaveText("approved");
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("an eval id from nowhere is the API's refusal, not a blank page", async ({ page }) => {
  await page.goto("/evals/00000000-0000-7000-8000-000000000000/sign-off");
  await expect(page.getByRole("heading", { name: "That did not load" })).toBeVisible();
  await expect(page.locator(".panel.problem")).toBeVisible();
});
