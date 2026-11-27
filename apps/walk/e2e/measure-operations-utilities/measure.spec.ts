import { expect, test } from "@playwright/test";

// Measure: the plan gate, the instrument panels and their links, and a failed read. Each panel
// reads the server's own action; no number is computed on the client.

test.describe.configure({ mode: "serial" });

const NAMES = [
  "evals.list",
  "benchmark_suites.list",
  "prompt_audits.list",
  "inference_calls.list",
  "metrics.usage",
];

function json(route: any, body: unknown) {
  return route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ output: body }) });
}

test("without any instrument the screen is the plan gate naming them", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => !NAMES.includes(entry.name));
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await page.goto("/measure");
  await expect(page.getByRole("heading", { name: "Measure is planned" })).toBeVisible();
  await expect(page.getByText(/None of evals\.list/)).toBeVisible();
});

test("the panels read their instruments and link to the owning screens", async ({ page }) => {
  await page.route("**/api/actions/evals.list/invocations", (route: any) => json(route, { items: [{}, {}] }));
  await page.route("**/api/actions/benchmark_suites.list/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/prompt_audits.list/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/inference_calls.list/invocations", (route: any) => json(route, { items: [{}] }));
  await page.route("**/api/actions/metrics.usage/invocations", (route: any) =>
    json(route, { level: "tenant", rows: [{ calls: 3 }] }),
  );
  await page.goto("/measure");
  await expect(page.getByRole("heading", { name: "Measure", level: 1 })).toBeVisible();
  const table = page.getByTestId("measure-instruments");
  await expect(table).toContainText("Evals");
  await expect(table).toContainText("2");
  await expect(page.getByTestId("measure-evals-list")).toContainText("2 row(s) read");
  await expect(page.getByRole("link", { name: "Open evals" })).toBeVisible();
});

test("a failed read is the problem panel for that instrument", async ({ page }) => {
  await page.route("**/api/actions/evals.list/invocations", (route: any) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({ title: "Internal Server Error", status: 500, detail: "the evals read failed" }),
    }),
  );
  await page.route("**/api/actions/benchmark_suites.list/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/prompt_audits.list/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/inference_calls.list/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/metrics.usage/invocations", (route: any) => json(route, { level: "tenant", rows: [] }));
  await page.goto("/measure");
  await expect(page.getByTestId("measure-evals-list")).toContainText("the evals read failed");
});
