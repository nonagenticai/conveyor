import { expect, test } from "@playwright/test";

// Benchmarks: the plan gate, the suites and runs, a run's detail, the empty suites and a read
// failure.

test.describe.configure({ mode: "serial" });

const SUITE = { id: "00000000-0000-4000-8000-000000000d01", slug: "verified50-v1", name: "verified50-v1", title: "verified50-v1", swebench_version: "1", version: "1", status: "active" };
const RUN = {
  id: "00000000-0000-4000-8000-000000000d02",
  suite_id: SUITE.id,
  arm: "baseline",
  score: 0.82,
  status: "succeeded",
  name: "run-1",
};

function json(route: any, body: unknown) {
  return route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ output: body }) });
}

test("without benchmark_suites.list the screen is the plan gate naming it", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => entry.name !== "benchmark_suites.list");
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await page.goto("/evals/benchmarks");
  await expect(page.getByRole("heading", { name: "Benchmarks is planned" })).toBeVisible();
  await expect(page.getByText(/benchmark_suites\.list is not in this role's manifest/)).toBeVisible();
});

test("the suites and runs render", async ({ page }) => {
  await page.route("**/api/actions/benchmark_suites.list/invocations", (route: any) => json(route, { items: [SUITE] }));
  await page.route("**/api/actions/benchmark_runs.list/invocations", (route: any) => json(route, { items: [RUN] }));
  await page.goto("/evals/benchmarks");
  await expect(page.getByRole("heading", { name: "Benchmarks", level: 1 })).toBeVisible();
  await expect(page.getByTestId("benchmarks-suites")).toContainText("verified50-v1");
  await expect(page.getByTestId("benchmarks-runs")).toContainText("baseline");
  await expect(page.getByRole("button", { name: "Start a run" })).toBeVisible();
});

test("a run's detail renders its score", async ({ page }) => {
  await page.route("**/api/actions/benchmark_runs.get/invocations", (route: any) => json(route, RUN));
  await page.goto(`/evals/benchmarks/run/${RUN.id}`);
  await expect(page.getByTestId("benchmark-run")).toContainText("baseline");
  await expect(page.getByRole("button", { name: "Compare" })).toBeVisible();
});

test("the empty suites is the app's own message", async ({ page }) => {
  await page.route("**/api/actions/benchmark_suites.list/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/benchmark_runs.list/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/evals/benchmarks");
  await expect(page.getByText(/No benchmark suite in this tenant/)).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route("**/api/actions/benchmark_suites.list/invocations", (route: any) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({ title: "Internal Server Error", status: 500, detail: "the suites read failed" }),
    }),
  );
  await page.goto("/evals/benchmarks");
  await expect(page.locator(".problem")).toContainText("the suites read failed");
});
