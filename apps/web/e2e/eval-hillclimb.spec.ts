import { expect, test } from "@playwright/test";

import { decideAsReviewer, invoke, isDevAuth } from "./helpers";

// Screen 21 — the hillclimb. A climb works one eval's instrument under a goal, a noise band and
// a stopping condition, and its rounds are the visible record. This spec builds its own ready
// instrument (the same gates a run needs) so it never depends on, or changes, the seeded eval.

test("a climb starts, lists, opens and stops, and an empty eval says so", async ({
  page,
  request,
}) => {
  test.skip(
    !isDevAuth(),
    "the deployed suite logs in as one principal: it cannot request and decide both eval gates",
  );
  test.setTimeout(120_000);
  const stamp = Date.now();

  // A ready instrument: cases and metrics first, then the two sign-offs, then the split draw —
  // the order the engine's own gate names, because the digest hashes the cases and metrics.
  const created = await invoke<{ id: string }>(request, "evals.create", {
    name: `e2e climb ${stamp}`,
    subject_kind: "completions",
    entry_action: "inference.chat",
    model_ref: "stub-model",
    inputs_schema: { type: "object", required: ["messages"] },
  });
  const version = await invoke<{ id: string }>(request, "eval_versions.create", {
    eval_id: created.id,
  });
  const cases = Array.from({ length: 20 }, (_, i) => ({
    input: JSON.stringify({
      messages: [{ role: "user", content: `e2e climb case ${stamp} #${i}` }],
    }),
    expected: "Stub answer: the copilot path works.",
    tags: [i < 14 ? "train" : "test"],
  }));
  await invoke(request, "eval_cases.import", { version_id: version.id, cases });
  await invoke(request, "eval_metrics.create", {
    version_id: version.id,
    key: "exact",
    role: "quality",
    kind: "binary",
    label: "exact",
    grader_kind: "programmatic",
    grader: { expect: "exact" },
  });
  for (const gate of ["eval_inputs", "eval_grading"]) {
    const approval = await invoke<{ id: string }>(request, "eval_signoffs.request", {
      eval_id: created.id,
      gate,
      ask: `e2e climb ${gate}`,
    });
    await decideAsReviewer(request, "eval_signoffs.decide", approval.id);
  }
  await invoke(request, "evals.draw_split", { eval_id: created.id, train_ratio: 0.7 });

  // Empty: this eval has no climb yet.
  await page.goto(`/evals/${created.id}/climb`);
  await expect(page.getByText("No climb on this eval.")).toBeVisible();

  // Start through the screen's own form. The noise band must sit above the measured floor for
  // six held-out cases — the action reports 0.4082 (1/sqrt(6)) — so 0.45 is above it.
  await page.getByLabel("Target metric").fill("exact");
  await page.getByLabel("Noise band").fill("0.45");
  await page.getByRole("button", { name: "Start climb" }).click();
  const row = page.locator("table.wtbl tbody tr", { hasText: "exact" });
  await expect(row).toHaveCount(1);
  await expect(row.locator(".pill")).toHaveText("planned");

  // The action's own list agrees with the rendered row.
  const climbs = await invoke<{ items: { id: string; target: string; status: string }[] }>(
    request,
    "eval_climbs.list",
    { eval_id: created.id },
  );
  expect(climbs.items.map((climb) => climb.target)).toContain("exact");

  // Open it: the round view renders, and with no rounds it says so rather than a blank table.
  await page.getByRole("button", { name: "Rounds" }).first().click();
  await expect(page.getByText("This climb has no rounds yet.")).toBeVisible();

  // Stop it through the screen; the row's status is the server's.
  await page.getByRole("button", { name: "Stop" }).first().click();
  await expect(row.locator(".pill")).toHaveText("stopped");
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("an eval id from nowhere is the API's refusal, not a blank page", async ({ page }) => {
  await page.goto("/evals/00000000-0000-7000-8000-000000000000/climb");
  await expect(page.getByRole("heading", { name: "That did not load" })).toBeVisible();
  await expect(page.locator(".panel.problem")).toBeVisible();
});
