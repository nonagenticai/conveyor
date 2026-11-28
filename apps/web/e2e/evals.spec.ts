import { expect, test } from "./fixture";

import { decideAsReviewer, invoke, isDevAuth, waitForReportRun } from "./helpers";

// The seeded eval (dev-a, `seed.rs::measure_improve`) has one version, six cases (4 train /
// 2 test), two metrics (`correct`, `grounded`), a completed baseline run with a result per
// case, `eval_inputs` approved and `eval_grading` pending, and one planned climb.
test("the report derives its numbers from the run's result rows", async ({ page, request }) => {
  await page.goto("/evals");
  await expect(page.getByRole("heading", { name: "Evals" })).toBeVisible();
  await page.getByRole("link", { name: "bug-fix judge" }).click();
  await expect(page).toHaveURL(/\/evals\/[0-9a-f-]+\/report/);

  // The screen renders the eval's newest run; wait for that run to be complete (the report
  // exists only then), then the reload is the screen read (#733).
  const evalId = page.url().split("/")[4];
  await waitForReportRun(request, evalId);
  await page.reload();

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

test("cases, sign-off and climb tabs read their real actions", async ({ page, request }) => {
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

  // Alice requested the grading gate, so the console shows it but refuses her the decision
  // (a sign-off is never self-allowed: its gate name is not a destructive action).
  await expect(grading.getByRole("button", { name: "Approve" })).toBeDisabled();
  await expect(
    grading.getByText("You requested this gate; another person must decide it."),
  ).toBeVisible();

  // Only the dev seed has a second dev-a admin to decide it.
  test.skip(!isDevAuth(), "the deployed suite has one OIDC principal, so nobody else can decide");

  const evalId = url.split("/").pop();
  const pending = await invoke<{
    items: { id: string; gate: string; eval_id: string | null }[];
  }>(request, "approvals.list", { status: "pending", limit: 100 });
  const gradingGate = pending.items.find(
    (row) => row.gate === "eval_grading" && row.eval_id === evalId,
  );
  if (!gradingGate) {
    throw new Error("the seeded eval_grading gate is not pending");
  }
  await decideAsReviewer(request, "eval_signoffs.decide", gradingGate.id);

  // The reviewer's decision is a write from another session; the reloaded tab is the read.
  await page.reload();
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
  test.setTimeout(120_000);
  // The seeded eval is named, never `evals.list[0]`: the authoring test above creates evals,
  // and a positional pick would report on the one it just made.
  await page.goto("/evals");
  await expect(page.getByRole("heading", { name: "Evals" })).toBeVisible();
  await page.getByRole("link", { name: "bug-fix judge" }).click();
  await expect(page).toHaveURL(/\/evals\/[0-9a-f-]+\/report/);

  // The screen renders the eval's newest run (`eval_runs.list`'s first row). Wait until that
  // run is complete — a queued run has no report, and the page does not refetch by itself —
  // then the reload is the screen read (#733).
  const evalId = page.url().split("/")[4];
  await waitForReportRun(request, evalId);
  await page.reload();

  // (a) The tiles carry `eval_runs.report`'s numbers: the case means over the six seeded
  // grades are correct 0.58 and grounded 0.68 — the same figures the browser used to compute,
  // now derived by the server — and the noise floor is the held-out sample's 1/sqrt(2·1).
  await expect(page.locator(".tile", { hasText: "Mean correct" }).locator(".tv")).toHaveText("0.58");
  await expect(page.locator(".tile", { hasText: "Mean grounded" }).locator(".tv")).toHaveText("0.68");
  await expect(page.locator(".tile", { hasText: /^Results/ }).locator(".tv")).toHaveText("6");
  await expect(page.locator(".tile", { hasText: "Graded" }).locator(".tv")).toHaveText("6");
  await expect(page.getByText("Noise floor:")).toBeVisible();
  await expect(page.getByText("Headroom:")).toBeVisible();

  // (a2) Provenance, not just arithmetic: those are the same numbers `eval_runs.report`
  // returns for the run the screen is showing. A browser-side recomputation would agree with
  // itself and could still disagree with the server; this pins the server's own means.
  const seededRuns = await invoke<{ items: { id: string }[] }>(request, "eval_runs.list", {
    eval_id: evalId,
    limit: 1,
  });
  const seededReport = await invoke<{ metrics: { key: string; mean: number | null }[] }>(
    request,
    "eval_runs.report",
    { run_id: seededRuns.items[0].id },
  );
  const serverMean = (key: string) =>
    seededReport.metrics.find((m) => m.key === key)?.mean?.toFixed(2);
  // The same two decimals the tiles show, from the server's unrounded means (0.5833…, 0.6833…).
  expect(serverMean("correct")).toBe("0.58");
  expect(serverMean("grounded")).toBe("0.68");

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

  // (d) The cost-goal/guardrail controls are `eval_climbs.start`'s own input: the schema takes
  // `goal{target, direction, hold[]}` (a `cost_usd` target with at least one quality hold) and
  // `cost_ceiling_cents`. One eval carries one open climb, so the seeded climb stops first.
  const seededRow = page.locator("table.wtbl tbody tr", { hasText: "correct" });
  await seededRow.getByRole("button", { name: "Stop" }).click();
  // Every climb write refreshes the instrument and rebuilds the tab; a screen read that lands
  // mid-rebuild sees an empty or stale table (measured on a fresh database, #733). Prove the
  // write on the climb list first, then the reload is the screen read.
  await expect
    .poll(
      async () => {
        const list = await invoke<{ items: { target: string; status: string }[] }>(
          request,
          "eval_climbs.list",
          { eval_id: evalId },
        );
        return list.items.find((row) => row.target === "correct")?.status;
      },
      { timeout: 30_000 },
    )
    .toBe("stopped");
  await page.reload();
  const costRow = page.locator("table.wtbl tbody tr", { hasText: "cost_usd" });
  await expect(seededRow.locator(".pill")).toHaveText("stopped");

  await page.getByLabel("Goal kind").selectOption("cost");
  await page.getByLabel("Guardrail metric").fill("correct");
  await page.getByLabel("Guardrail floor").fill("0.5");
  await page.getByLabel("Spend ceiling (cents)").fill("500");
  // Six seeded cases split 4/2: the two held-out cases put the floor at 1/sqrt(2) = 0.7071.
  await page.getByLabel("Noise band").fill("0.75");
  await page.getByRole("button", { name: "Start climb" }).click();

  // Same as the stop: prove the start on the climb list, then reload as the screen read.
  await expect
    .poll(
      async () => {
        const list = await invoke<{ items: { target: string }[] }>(
          request,
          "eval_climbs.list",
          { eval_id: evalId },
        );
        return list.items.filter((row) => row.target === "cost_usd").length;
      },
      { timeout: 30_000 },
    )
    .toBe(1);
  await page.reload();
  await expect(costRow).toHaveCount(1);
  await expect(costRow.locator(".pill")).toHaveText("planned");
  await expect(costRow).toContainText("correct ≥ 0.50");
  await expect(costRow).toContainText("spend ≤ $5.00");

  // The start call really carried them: the list is the server's read of the stored goal.
  const climbs = await invoke<{
    items: {
      id: string;
      target: string;
      direction: string;
      goal: { hold: { metric: string; min: number }[] };
      cost_ceiling_cents: number | null;
    }[];
  }>(request, "eval_climbs.list", { eval_id: evalId });
  const costClimb = climbs.items.find((row) => row.target === "cost_usd");
  if (!costClimb) {
    throw new Error("the cost climb was not stored");
  }
  expect(costClimb.direction).toBe("lower");
  expect(costClimb.goal.hold).toEqual([{ metric: "correct", min: 0.5 }]);
  expect(costClimb.cost_ceiling_cents).toBe(500);

  // The plan gate is round 1's authorisation; a reviewer decides it.
  const climbGates = await invoke<{
    items: { id: string; gate: string; eval_id: string | null }[];
  }>(request, "approvals.list", { status: "pending", limit: 100 });
  const planGate = climbGates.items.find(
    (row) => row.gate === "eval_climb_plan" && row.eval_id === evalId,
  );
  if (!planGate) {
    throw new Error("the cost climb's plan gate is not pending");
  }
  await decideAsReviewer(request, "eval_signoffs.decide", planGate.id);

  // (e) The round form's `change_md`, `target` and `prediction` reach the action: a prediction
  // naming a metric other than the goal is refused (so a recorded one was parsed and matched),
  // the round is recorded, and the per-round table shows it.
  await costRow.getByRole("button", { name: "Rounds" }).click();
  await expect(page.getByText("This climb has no rounds yet.")).toBeVisible();
  await page.getByLabel("Round climb").selectOption(costClimb.id);
  await page.getByLabel("Round target").fill("prompt:e2e");
  await page.getByLabel("What changed").fill("e2e: drop the redundant system preamble");
  await page.getByLabel("Lever").fill("caching_health");
  await page.getByLabel("Prediction").fill(
    JSON.stringify({
      metric: "correct",
      direction: "lower",
      interval: [0.75, 0.9],
      falsifier: "e2e: the wrong hypothesis",
    }),
  );
  await page.getByRole("button", { name: "Record round" }).click();
  await expect(page.locator('.callout[role="status"]')).toContainText("cost_usd");
  await page.getByLabel("Prediction").fill(
    JSON.stringify({
      metric: "cost_usd",
      direction: "lower",
      interval: [0.75, 0.9],
      falsifier: "e2e: spend does not fall",
    }),
  );
  await page.getByRole("button", { name: "Record round" }).click();
  await expect(page.locator('.callout[role="status"]')).toContainText("Round 1 recorded");

  // The recording is the write; prove it on the climb's own read first. Recording also refreshes
  // the instrument, and the tab's rebuild can empty the rounds table right after it filled —
  // measured on a fresh database (#733): the count read 1 and the very next read found no rows.
  // The reload sequences the screen read after the rebuild; the read gesture is the climb's own
  // "Rounds" button, which then carries the round the form just recorded.
  await expect
    .poll(
      async () => {
        const climb = await invoke<{ rounds: { round: number }[] }>(request, "eval_climbs.get", {
          id: costClimb.id,
        });
        return climb.rounds.length;
      },
      { timeout: 30_000 },
    )
    .toBe(1);
  await page.reload();
  await costRow.getByRole("button", { name: "Rounds" }).click();
  const roundRows = page.locator("table.wtbl", { hasText: "Variant" }).locator("tbody tr");
  await expect(roundRows).toHaveCount(1);
  await expect(roundRows.first()).toContainText("v1");

  const opened = await invoke<{
    rounds: { round: number; change_md: string | null; status: string }[];
  }>(request, "eval_climbs.get", { id: costClimb.id });
  expect(opened.rounds[0].round).toBe(1);
  expect(opened.rounds[0].change_md).toBe("e2e: drop the redundant system preamble");

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

// P3.2: detail rows page past one page and the whole-run score never moves.
//
// The screen asks `eval_results.list` for 50 rows at a time and follows its `next_cursor`; the
// tiles come from `eval_runs.report`, which counts case slots and grades, never a page. The
// instrument is built through the real actions in the order the engine's own gate names —
// cases and metrics first, then the two sign-offs, then the split draw — because the version
// digest hashes the cases and metrics, and signing earlier voids the signature the moment the
// next row lands. 56 > the 50-row page, so one Load more is the whole proof.
test("results page past one page and the whole-run score does not change", async ({
  page,
  request,
}) => {
  test.skip(
    !isDevAuth(),
    "the deployed suite logs in as one principal: it cannot request and decide both eval gates",
  );
  test.setTimeout(240_000);
  const stamp = Date.now();

  const created = await invoke<{ id: string }>(request, "evals.create", {
    name: `e2e paging ${stamp}`,
    subject_kind: "completions",
    entry_action: "inference.chat",
    // The dev stack's provider double answers as `stub-model`; a run whose served model
    // disagrees with the declared one is refused as `served_model_mismatch`.
    model_ref: "stub-model",
    // A non-empty schema makes the case input the action's request verbatim; `inference.chat`
    // requires `messages`, so each case carries the router's own shape.
    inputs_schema: { type: "object", required: ["messages"] },
  });
  const version = await invoke<{ id: string }>(request, "eval_versions.create", {
    eval_id: created.id,
  });
  const cases = Array.from({ length: 56 }, (_, i) => ({
    input: JSON.stringify({
      messages: [{ role: "user", content: `e2e paging case ${stamp} #${i}` }],
    }),
    expected: "Stub answer: the copilot path works.",
    tags: [i < 40 ? "train" : "test"],
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
      ask: `e2e paging ${gate}`,
    });
    await decideAsReviewer(request, "eval_signoffs.decide", approval.id);
  }
  await invoke(request, "evals.draw_split", { eval_id: created.id, train_ratio: 0.7 });
  const started = await invoke<{ run: { id: string } }>(request, "eval_runs.start", {
    eval_id: created.id,
  });

  // The worker records one result per case. `graded` is the report's own count, so this waits
  // on the same figure the screen will render.
  await expect
    .poll(
      async () => {
        const report = await invoke<{ graded: number }>(request, "eval_runs.report", {
          run_id: started.run.id,
        });
        return report.graded;
      },
      { timeout: 180_000, intervals: [1000, 2000, 3000] },
    )
    .toBe(56);

  await page.goto(`/evals/${created.id}/report`);
  const resultRows = page.locator('table[aria-label="Run results"] tbody tr');
  await expect(resultRows).toHaveCount(50);
  const score = page.locator(".tile", { hasText: "Mean exact" }).locator(".tv");
  const firstPageScore = await score.innerText();

  await page.getByRole("button", { name: "Load more" }).click();
  await expect(resultRows).toHaveCount(56);
  await expect(score).toHaveText(firstPageScore);

  // The whole-run figure is the server's count of slots, not the rows on screen.
  const report = await invoke<{ cases: number; graded: number }>(
    request,
    "eval_runs.report",
    { run_id: started.run.id },
  );
  expect(report.cases).toBe(56);
  expect(report.graded).toBe(56);
  await expect(page.locator(".tile", { hasText: /^Results/ }).locator(".tv")).toHaveText("56");
  await expect(page.locator(".problem")).toHaveCount(0);
});
