import { expect, test, type Page } from "./fixture";

import { createBoardCard, invoke } from "./helpers";

const baseURL = process.env.BASE_URL ?? "http://127.0.0.1:8931";

/** Swap the dev cookie. The file's storageState is alice; fr-test is the platform admin. */
async function as(page: Page, login: string) {
  const host = new URL(baseURL).hostname;
  await page.context().addCookies([
    { name: "conveyor_dev", value: login, domain: host, path: "/" },
  ]);
}

test("the queue shows a run this test created, filtered by its state", async ({ page, request }) => {
  // Happy path, seeded through the real actions: create the run, cancel it, then read it back
  // through the screen's own filter. Cancelling fixes the asserted state — `runs.cancel` is
  // legal from every live state, and `run.execute` never completes a card run — so a worker
  // running or not cannot change the pill under the assertion.
  const fixture = await createBoardCard(request, `run-queue-${Date.now()}`);
  const run = await invoke<{ id: string }>(request, "runs.create", {
    card_id: fixture.card.id,
  });
  await invoke(request, "runs.cancel", { id: run.id });

  await page.goto("/runs");
  await expect(page.getByRole("heading", { name: "Runs" })).toBeVisible();

  // The shell also has a tenant <select>; this is the screen's own filter.
  const filter = page.locator(".screen-head select");
  await filter.selectOption("cancelled");
  await expect(filter).toHaveValue("cancelled");

  // The row prints the run's full id in its `.adv` cell, so match on all of it: an 8-char
  // prefix is shared by every run created in the same millisecond window and resolves to
  // several rows (strict-mode violation, measured 2026-11-27).
  const row = page.locator(".row-card", { hasText: run.id });
  await expect(row).toBeVisible();
  await expect(row.locator(".pill")).toHaveText("cancelled");
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("a tenant with no runs says so instead of rendering a blank queue", async ({ page }) => {
  // fr-test's platform tenants carry no project work: `processes.spec.ts` creates its runs in
  // dev-b, and the copilot's kata/execution turn writes a conversation, not a run.
  await as(page, "fr-test");
  await page.goto("/runs");
  await expect(page.getByRole("heading", { name: "Runs" })).toBeVisible();
  await expect(page.getByText("No runs in this filter.")).toBeVisible();
  await expect(page.locator(".row-card")).toHaveCount(0);
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("a failed queue read is the screen's error panel", async ({ page }) => {
  // Every filter the screen offers is a real `RunStatus`, so no input the UI can send is
  // refused (`v1-s12-1`: the old "Expired" entry was removed). The genuine error state is the
  // transport/BFF failure path: the 503 below is injected for that one call and every other
  // request, including the shell's boot, hits the real API.
  await page.route("**/api/actions/runs.list/invocations", (route) =>
    route.fulfill({
      status: 503,
      contentType: "application/problem+json",
      body: JSON.stringify({
        type: "internal",
        title: "Service Unavailable",
        status: 503,
        detail: "the run store is unavailable",
      }),
    }),
  );

  await page.goto("/runs");
  await expect(page.getByRole("heading", { name: "That did not load" })).toBeVisible();
  await expect(page.locator(".panel.problem")).toContainText("the run store is unavailable");
});
