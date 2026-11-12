import { expect, test, type APIRequestContext } from "@playwright/test";

import { invoke } from "./helpers";

// The seeded eval this spec asserts about; authored evals persist (there is no delete action),
// so "newest first" is not the fixture once any authoring test has run.
async function firstEval(request: APIRequestContext): Promise<string> {
  const page = await invoke<{ items: { id: string; name: string }[] }>(
    request,
    "evals.list",
    { limit: 50 },
  );
  const seeded = page.items.find((item) => item.name.startsWith("bug-fix judge"));
  return (seeded ?? page.items[0]).id;
}

test("a health audit runs, reports its controls and a finding is decided", async ({
  page,
  request,
}) => {
  const id = await firstEval(request);
  await page.goto(`/evals/${id}/audit`);
  await expect(page.getByRole("button", { name: "Run audit" })).toBeVisible();

  await page.getByRole("button", { name: "Run audit" }).click();
  await expect(page.getByText("Audit queued")).toBeVisible();

  // The worker writes the findings; the screen reads on refresh. Poll the DETAIL (the findings
  // this audit wrote), not any earlier row: a stale in-flight fetch can otherwise satisfy a row
  // count while the selected audit is still queued.
  await expect
    .poll(
      async () => {
        await page.getByRole("button", { name: "Refresh" }).click();
        return page.locator(".finding").count();
      },
      { timeout: 30_000, intervals: [500, 1000, 2000] },
    )
    .toBeGreaterThan(0);
  await expect(page.locator("section.panel", { hasText: "Controls" })).toBeVisible();
  // The seeded eval audits to two findings (judge_model_missing, no_perf_metric); a clean
  // panel is not accepted as a pass for this test.
  await expect(page.locator(".finding").first()).toBeVisible({ timeout: 30_000 });

  // Decide the first open finding through the real action; the row records the decision.
  const accept = page.getByRole("button", { name: "Accept", exact: true });
  await expect(accept.first()).toBeVisible();
  await accept.first().click();
  await expect(page.locator(".pill", { hasText: "accepted" }).first()).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("the eval audit tab has no horizontal page scroll at 1280 and 390", async ({
  page,
  request,
}) => {
  const id = await firstEval(request);
  await page.goto(`/evals/${id}/audit`);
  await expect(page.getByRole("button", { name: "Run audit" })).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `width ${width}`).toBeLessThanOrEqual(0);
  }
});
