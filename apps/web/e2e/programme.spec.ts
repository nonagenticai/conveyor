import { expect, test, type Page } from "./fixture";

import { authFor, invoke, isDevAuth } from "./helpers";

const baseURL = process.env.BASE_URL ?? "http://127.0.0.1:8931";

/** Swap the dev cookie. The file's storageState is alice (admin of dev-a). */
async function as(page: Page, login: string) {
  const host = new URL(baseURL).hostname;
  await page.context().addCookies([
    { name: "conveyor_dev", value: login, domain: host, path: "/" },
  ]);
}

// T18-1 P2: the Programme destination reads the tenant's programmes, never a fixture.

test("a created programme renders from the live read", async ({ page, request }) => {
  const name = `E2E programme ${Date.now()}`;
  await invoke(request, "programmes.create", { name });

  await page.goto("/programme");
  await expect(page.getByRole("heading", { name: "Programme" })).toBeVisible();
  await expect(page.getByRole("cell", { name })).toBeVisible();
  await expect(page.getByText("No programmes in this tenant yet.")).toHaveCount(0);
});

// #848 — the T18 §10 remainder: empty, error and one server refusal beside the happy path.
// The empty state is `fr-test`'s kata/execution tenant, which holds no programme and stays
// empty for the whole suite — dev-b is written to by processes.spec.ts:138.
test("an empty tenant says so, never a fixture", async ({ page }) => {
  test.skip(!isDevAuth(), "the deployed suite has one principal; fr-test is a dev seed");
  await as(page, "fr-test");
  await page.goto("/programme");
  await expect(page.getByRole("heading", { name: "Programme", exact: true })).toBeVisible();
  await expect(page.getByText("No programmes in this tenant yet.")).toBeVisible();
  await expect(page.locator("table.table")).toHaveCount(0);
});

test("a failed programme read is the error panel, with a retry", async ({ page }) => {
  await page.route(/\/api\/actions\/programmes\.list\/invocations/, (route) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({ detail: "forced by the spec" }),
    }),
  );
  await page.goto("/programme");
  const panel = page.locator("div.panel.problem[role=alert]");
  await expect(panel.getByRole("heading", { name: "That did not load" })).toBeVisible();
  await expect(panel).toContainText("forced by the spec");
  await expect(panel.getByRole("button", { name: "Try again" })).toBeVisible();
});

// The engine refuses a second programme with the same name: a 409 from the store, not a
// client-side dedupe. The spec drives the same action the screen's create path uses.
test("a duplicate programme is refused by the server", async ({ request }) => {
  const name = `E2E duplicate ${Date.now()}`;
  await invoke(request, "programmes.create", { name });
  const response = await request.post("/api/actions/programmes.create/invocations", {
    headers: { ...authFor(), "content-type": "application/json" },
    data: { input: { name } },
  });
  expect(response.status()).toBe(409);
  expect(await response.text()).toContain("duplicate");
});
