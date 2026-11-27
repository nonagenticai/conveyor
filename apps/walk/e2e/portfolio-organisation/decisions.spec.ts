import { expect, test } from "@playwright/test";

// Decisions: the ladder's recorded decisions, the policies behind them, the Approvals link, and
// the empty/loading/error/gate states.

test.describe.configure({ mode: "serial" });

const DECISION = {
  kind: "ladder",
  rung: "hold",
  question: "Is the evidence complete?",
  answer: "no",
  project_id: "00000000-0000-4000-8000-000000000040",
};
const POLICY = { kind: "threshold", key: "cost", status: "active", version: 2 };

function json(route: any, body: unknown) {
  return route.fulfill({
    status: 200,
    contentType: "application/json",
    body: JSON.stringify({ output: body }),
  });
}

test("the recorded decisions and policies render, with the Approvals link", async ({ page }) => {
  await page.route("**/api/actions/ladder.list/invocations", (route) =>
    json(route, { items: [DECISION] }),
  );
  await page.route("**/api/actions/decision_policies.list/invocations", (route) =>
    json(route, { items: [POLICY] }),
  );
  await page.goto("/decisions");
  await expect(page.getByRole("heading", { name: "Decisions", level: 1 })).toBeVisible();
  await expect(page.getByTestId("decisions-table")).toContainText("Is the evidence complete?");
  await expect(page.getByTestId("decisions-policies")).toContainText("cost");
  await expect(page.getByRole("link", { name: "Approvals" })).toBeVisible();
});

test("the empty sections are the app's own messages", async ({ page }) => {
  await page.route("**/api/actions/ladder.list/invocations", (route) => json(route, { items: [] }));
  await page.route("**/api/actions/decision_policies.list/invocations", (route) =>
    json(route, { items: [] }),
  );
  await page.goto("/decisions");
  await expect(page.getByText("No ladder decision in this tenant yet.", { exact: true })).toBeVisible();
  await expect(page.getByText("No decision policy in this tenant yet.", { exact: true })).toBeVisible();
});

test("the loading state is shown while the read is in flight", async ({ page }) => {
  await page.route("**/api/actions/ladder.list/invocations", async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 800));
    await route.continue();
  });
  await page.goto("/decisions");
  await expect(page.getByText("Loading…", { exact: true })).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route("**/api/actions/ladder.list/invocations", (route) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({
        title: "Internal Server Error",
        status: 500,
        detail: "the decisions read failed",
      }),
    }),
  );
  await page.goto("/decisions");
  await expect(page.locator(".problem")).toContainText("the decisions read failed");
});

test("a signed-out visit is the sign-in gate", async ({ page }) => {
  await page.goto("/decisions");
  await page.evaluate(() => {
    document.cookie = "conveyor_dev=; Max-Age=0; Path=/";
  });
  await page.goto("/decisions");
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
});
