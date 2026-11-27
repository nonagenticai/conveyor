import { expect, test } from "@playwright/test";

import { invoke, waitForReportRun } from "./helpers";

// Screen 17 — the dedicated eval report. The seed's `bug-fix judge` eval has one completed run
// with six graded cases; the figures are `eval_runs.report`'s, the rows are that run's results,
// and a row opens the recorded transcript. Empty and error are this route's own states — the
// evals list's report numbers are #26's concern, not this spec's.

test("a finished run renders from eval_runs.report, with its transcript", async ({
  page,
  request,
}) => {
  await page.goto("/evals");
  await expect(page.getByRole("heading", { name: "Evals" })).toBeVisible();
  await page.getByRole("link", { name: "bug-fix judge" }).click();
  await expect(page).toHaveURL(/\/evals\/[0-9a-f-]+\/report/);
  const evalId = page.url().split("/")[4];

  // The screen renders the eval's newest run; wait for it to be complete, then the reload is
  // the screen read (#733). A queued run has no report and the page does not refetch by itself.
  await waitForReportRun(request, evalId);
  await page.reload();

  // The screen is the server's report: fetch the same run's `eval_runs.report` and assert the
  // tiles and the row count against it, so a browser-side recomputation cannot pass this.
  const runs = await invoke<{ items: { id: string }[] }>(request, "eval_runs.list", {
    eval_id: evalId,
    limit: 1,
  });
  const report = await invoke<{
    cases: number;
    graded: number;
    metrics: { key: string; mean: number | null }[];
  }>(request, "eval_runs.report", { run_id: runs.items[0].id });
  const mean = (key: string) =>
    report.metrics.find((m) => m.key === key)?.mean?.toFixed(2) ?? "";

  await expect(page.locator(".tile", { hasText: "Mean correct" }).locator(".tv")).toHaveText(
    mean("correct"),
  );
  await expect(page.locator(".tile", { hasText: "Mean grounded" }).locator(".tv")).toHaveText(
    mean("grounded"),
  );
  await expect(page.locator(".tile", { hasText: /^Results/ }).locator(".tv")).toHaveText(
    String(report.cases),
  );
  await expect(page.locator(".tile", { hasText: "Graded" }).locator(".tv")).toHaveText(
    String(report.graded),
  );

  const rows = page.locator('table[aria-label="Run results"] tbody tr');
  await expect(rows).toHaveCount(report.graded);

  // A row opens the recorded transcript through the real `eval_results.get`.
  await rows.first().click();
  await expect(page.getByText("Transcript")).toBeVisible();

  await expect(page.locator(".problem")).toHaveCount(0);
});

test("an eval with no run says so, and an id from nowhere is the API's refusal", async ({
  page,
  request,
}) => {
  // Empty: a fresh eval has no version and no run. The route says exactly that.
  const created = await invoke<{ id: string }>(request, "evals.create", {
    name: `e2e report empty ${Date.now()}`,
    subject_kind: "completions",
    entry_action: "inference.chat",
  });
  await page.goto(`/evals/${created.id}/report`);
  await expect(page.getByText("No run to report on yet.")).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);

  // Error: the eval does not resolve, so the screen renders the API's refusal panel.
  await page.goto("/evals/00000000-0000-7000-8000-000000000000/report");
  await expect(page.getByRole("heading", { name: "That did not load" })).toBeVisible();
  await expect(page.locator(".panel.problem")).toBeVisible();
});
