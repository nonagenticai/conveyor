import { expect, test } from "@playwright/test";

// Evals: the plan gate, the list, the detail's three tabs (cases, runs, audits) and a write's
// sheet, the empty list and a read failure.

test.describe.configure({ mode: "serial" });

const EVAL = {
  id: "00000000-0000-4000-8000-000000000c01",
  name: "Door decisions",
  title: "Door decisions",
  purpose: "Score the door decision quality",
  subject_kind: "card",
  entry_action: "cards.create",
  status: "active",
  current_version_id: "00000000-0000-4000-8000-000000000c02",
  created_at: "2026-11-01T09:00:00Z",
  updated_at: "2026-11-20T09:00:00Z",
};
const CASE = { id: "00000000-0000-4000-8000-000000000c03", name: "the door case", title: "the door case", key: "door-case", status: "active", input: {}, expected: {} };
const RUN = { id: "00000000-0000-4000-8000-000000000c04", name: "run-1", status: "succeeded", score: 0.9, eval_id: EVAL.id };
const AUDIT = { id: "00000000-0000-4000-8000-000000000c05", name: "audit-1", status: "succeeded", eval_id: EVAL.id };

function json(route: any, body: unknown) {
  return route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ output: body }) });
}

test("without evals.list the screen is the plan gate naming it", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => entry.name !== "evals.list");
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await page.goto("/evals");
  await expect(page.getByRole("heading", { name: "Evals is planned" })).toBeVisible();
  await expect(page.getByText(/evals\.list is not in this role's manifest/)).toBeVisible();
});

test("the list renders each eval", async ({ page }) => {
  await page.route("**/api/actions/evals.list/invocations", (route: any) => json(route, { items: [EVAL] }));
  await page.goto("/evals");
  await expect(page.getByRole("heading", { name: "Evals", level: 1 })).toBeVisible();
  await expect(page.getByTestId("evals-list")).toContainText("Door decisions");
  await expect(page.getByRole("button", { name: "New eval" })).toBeVisible();
});

test("the detail's cases tab renders and the run write opens its sheet", async ({ page }) => {
  await page.route("**/api/actions/evals.get/invocations", (route: any) => json(route, EVAL));
  await page.route("**/api/actions/eval_cases.list/invocations", (route: any) => json(route, { items: [CASE] }));
  await page.goto(`/evals/${EVAL.id}`);
  await expect(page.getByRole("heading", { name: "Door decisions" })).toBeVisible();
  await expect(page.getByTestId("eval-cases")).toContainText("the door case");
  await page.getByRole("button", { name: "Start a run" }).click();
  const dialog = page.getByRole("dialog", { name: "Start a run" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Submit" })).toBeVisible();
});

test("the runs tab renders the eval's runs", async ({ page }) => {
  await page.route("**/api/actions/evals.get/invocations", (route: any) => json(route, EVAL));
  await page.route("**/api/actions/eval_runs.list/invocations", (route: any) => json(route, { items: [RUN] }));
  await page.goto(`/evals/${EVAL.id}/runs`);
  await expect(page.getByTestId("eval-runs")).toContainText("succeeded");
});

test("the audits tab renders the eval's audits", async ({ page }) => {
  await page.route("**/api/actions/evals.get/invocations", (route: any) => json(route, EVAL));
  await page.route("**/api/actions/eval_audits.list/invocations", (route: any) => json(route, { items: [AUDIT] }));
  await page.goto(`/evals/${EVAL.id}/audits`);
  await expect(page.getByTestId("eval-audits")).toContainText("audit-1");
});

test("the empty list is the app's own message", async ({ page }) => {
  await page.route("**/api/actions/evals.list/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/evals");
  await expect(page.getByText("No eval in this tenant yet.")).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route("**/api/actions/evals.list/invocations", (route: any) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({ title: "Internal Server Error", status: 500, detail: "the evals read failed" }),
    }),
  );
  await page.goto("/evals");
  await expect(page.locator(".problem")).toContainText("the evals read failed");
});
