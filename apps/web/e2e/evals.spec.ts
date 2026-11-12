import { expect, test } from "@playwright/test";

import { invoke } from "./helpers";

// The seeded eval (dev-a, `seed.rs::measure_improve`) has one version, six cases (4 train /
// 2 test), two metrics (`correct`, `grounded`), a completed baseline run with a result per
// case, `eval_inputs` approved and `eval_grading` pending, and one planned climb.
test("the report derives its numbers from the run's result rows", async ({ page }) => {
  await page.goto("/evals");
  await expect(page.getByRole("heading", { name: "Evals" })).toBeVisible();
  await page.getByRole("link", { name: "bug-fix judge" }).click();
  await expect(page).toHaveURL(/\/evals\/[0-9a-f-]+\/report/);

  // Means over the six grades: correct (1+0+1+0.5+1+0)/6 = 0.58, grounded 0.68.
  await expect(page.locator(".tile", { hasText: "Mean correct" }).locator(".tv")).toHaveText("0.58");
  await expect(page.locator(".tile", { hasText: "Mean grounded" }).locator(".tv")).toHaveText("0.68");
  await expect(page.locator(".tile", { hasText: /^Results/ }).locator(".tv")).toHaveText("6");
  // The per-tag breakdown is its own table; the per-case rows are the labelled one.
  const resultRows = page.locator('table[aria-label="Run results"] tbody tr');
  await expect(resultRows).toHaveCount(6);

  // A row opens the recorded transcript through the real `eval_results.get`.
  await resultRows.first().click();
  await expect(page.getByText("Transcript")).toBeVisible();

  // The held-out split filter is a request the server answers; with `evals:approve` it returns
  // the two test cases, and the table shrinks to them.
  await page.getByLabel("Split").selectOption("test");
  await expect.poll(async () => resultRows.count()).toBe(2);

  await expect(page.locator(".problem")).toHaveCount(0);
});

test("cases, sign-off and climb tabs read their real actions", async ({ page }) => {
  await page.goto("/evals");
  await page.getByRole("link", { name: "bug-fix judge" }).click();
  const url = page.url().replace(/\/report$/, "");

  // Cases: six rows, four train and two test pills.
  await page.goto(`${url}/cases`);
  await expect(page.getByRole("heading", { name: /bug-fix judge/ })).toBeVisible();
  await expect(page.locator("table.wtbl tbody tr")).toHaveCount(6);
  await expect(page.locator(".pill", { hasText: "train" })).toHaveCount(4);
  await expect(page.locator(".pill", { hasText: "test" })).toHaveCount(2);

  // Sign-off: the approved inputs gate and the pending grading gate, both listed by eval_id.
  await page.goto(`${url}/sign-off`);
  await expect(page.locator("tr", { hasText: "eval_inputs" }).locator(".pill")).toHaveText("approved");
  const grading = page.locator("tr", { hasText: "eval_grading" });
  await expect(grading.locator(".pill")).toHaveText("pending");
  await grading.getByRole("button", { name: "Approve" }).click();
  await expect(page.locator("tr", { hasText: "eval_grading" }).locator(".pill")).toHaveText("approved");

  // Climb: the seeded planned climb targets `correct`.
  await page.goto(`${url}/climb`);
  await expect(page.locator("table.wtbl tbody tr", { hasText: "correct" })).toHaveCount(1);

  // The Audit tab now starts a real audit; the Interop tab is the export/import pair.
  await page.goto(`${url}/audit`);
  await expect(page.getByRole("button", { name: "Run audit" })).toBeVisible();
  await page.goto(`${url}/interop`);
  await expect(page.getByRole("button", { name: "Export", exact: true })).toBeVisible();

  await expect(page.locator(".problem")).toHaveCount(0);
});

test("the report is the server's; the label and round controls hit their actions and rules", async ({
  page,
  request,
}) => {
  // The seeded eval is named, never `evals.list[0]`: the authoring test above creates evals,
  // and a positional pick would report on the one it just made.
  await page.goto("/evals");
  await expect(page.getByRole("heading", { name: "Evals" })).toBeVisible();
  await page.getByRole("link", { name: "bug-fix judge" }).click();
  await expect(page).toHaveURL(/\/evals\/[0-9a-f-]+\/report/);

  // (a) The tiles carry `eval_runs.report`'s numbers: the case means over the six seeded
  // grades are correct 0.58 and grounded 0.68 — the same figures the browser used to compute,
  // now derived by the server — and the noise floor is the held-out sample's 1/sqrt(2·1).
  await expect(page.locator(".tile", { hasText: "Mean correct" }).locator(".tv")).toHaveText("0.58");
  await expect(page.locator(".tile", { hasText: "Mean grounded" }).locator(".tv")).toHaveText("0.68");
  await expect(page.locator(".tile", { hasText: /^Results/ }).locator(".tv")).toHaveText("6");
  await expect(page.locator(".tile", { hasText: "Graded" }).locator(".tv")).toHaveText("6");
  await expect(page.getByText("Noise floor:")).toBeVisible();
  await expect(page.getByText("Headroom:")).toBeVisible();

  const url = page.url().replace(/\/report$/, "");

  // (b) `eval_results.label` is the human queue. Every seeded result is already `ok`, and a
  // result is only parked when a run's metric is human-graded — the seed has no parked row, so
  // the honest proof is the queue's own rule: the screen offers no Label control, and the
  // action refuses a non-parked row with 409 not_pending_label.
  await page.goto(`${url}/runs`);
  await page.getByRole("button", { name: "Results" }).first().click();
  await expect(page.getByLabel("Human score")).toHaveCount(0);
  const runs = await invoke<{ items: { id: string }[] }>(request, "eval_runs.list", {
    eval_id: url.split("/").pop(),
    limit: 1,
  });
  const results = await invoke<{ items: { id: string; status: string }[] }>(
    request,
    "eval_results.list",
    { run_id: runs.items[0].id, limit: 1 },
  );
  expect(results.items[0].status).toBe("ok");
  await expect(
    invoke(request, "eval_results.label", {
      result_id: results.items[0].id,
      human_score: 0.75,
    }),
  ).rejects.toThrow(/not_pending_label/);

  // (c) The round control calls `eval_climbs.round`, and the climb's own plan rule answers: the
  // seeded climb's `eval_climb_plan` gate is not approved, so the refusal is shown verbatim.
  // (An approved plan would record the round; the seed carries no approved plan gate.)
  await page.goto(`${url}/climb`);
  await page.getByLabel("What changed").fill("e2e: tighten the grounded rubric");
  await page.getByRole("button", { name: "Record round" }).click();
  await expect(page.locator('.callout[role="status"]')).toContainText(/plan/i);

  await expect(page.locator(".problem")).toHaveCount(0);
});

test("an instrument can be authored from the screen", async ({ page }) => {
  const stamp = Date.now();
  const name = `e2e eval ${stamp}`;
  const caseInput = `e2e case ${stamp}`;
  const metricKey = `e2e_metric_${stamp}`;

  // Create through the real `evals.create` form; the required three fields are the only
  // ones the form asks for by default.
  await page.goto("/evals");
  await expect(page.getByRole("heading", { name: "Evals" })).toBeVisible();
  await page.getByLabel("Eval name").fill(name);
  await page.getByRole("button", { name: "Create eval" }).click();
  await page.getByRole("link", { name }).click();
  await expect(page).toHaveURL(/\/evals\/[0-9a-f-]+\/report/);

  // A new eval has no version; the cases and metrics live on a frozen one.
  const url = page.url().replace(/\/report$/, "");
  await page.goto(`${url}/cases`);
  await page.getByRole("button", { name: "Freeze the next version" }).click();

  await page.getByLabel("Case input").fill(caseInput);
  await page.getByRole("button", { name: "Add case" }).click();
  await expect(
    page.locator("table.wtbl tbody tr", { hasText: caseInput }),
  ).toHaveCount(1);

  await page.getByLabel("Metric key").fill(metricKey);
  await page.getByLabel("Metric label").fill("e2e label");
  await page.getByRole("button", { name: "Add metric" }).click();
  await expect(
    page.locator(".panel", { hasText: "Metrics" }).locator("tbody tr", { hasText: metricKey }),
  ).toHaveCount(1);

  await expect(page.locator(".problem")).toHaveCount(0);
});
