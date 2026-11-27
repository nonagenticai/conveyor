import { expect, test } from "@playwright/test";

// Validation: every project's status and recorded evidence, the programme name resolution, and
// the empty/loading/error/gate states.

test.describe.configure({ mode: "serial" });

const PROJECT = {
  id: "00000000-0000-4000-8000-000000000050",
  key: "S1-VALIDATION",
  name: "S1 validation project",
  status: "active",
  programme_id: "00000000-0000-4000-8000-000000000051",
  meta: { phase: "B", evidence: "recorded" },
};

function json(route: any, body: unknown) {
  return route.fulfill({
    status: 200,
    contentType: "application/json",
    body: JSON.stringify({ output: body }),
  });
}

test("the project renders with its phase, evidence and programme name", async ({ page }) => {
  await page.route(/\/api\/projects\?limit=200$/, (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ items: [PROJECT] }),
    }),
  );
  await page.route("**/api/actions/programmes.list/invocations", (route) =>
    json(route, { items: [{ id: PROJECT.programme_id, name: "S1 programme" }] }),
  );
  await page.goto("/validation");
  await expect(page.getByRole("heading", { name: "Validation", level: 1 })).toBeVisible();
  const row = page.getByRole("row", { name: /S1 validation project/ });
  await expect(row).toContainText("B");
  await expect(row).toContainText("recorded");
  await expect(row).toContainText("S1 programme");
});

test("the empty list is the app's own message", async ({ page }) => {
  await page.route(/\/api\/projects\?limit=200$/, (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ items: [] }),
    }),
  );
  await page.goto("/validation");
  await expect(page.getByText("No project in this tenant yet.", { exact: true })).toBeVisible();
});

test("the loading state is shown while the read is in flight", async ({ page }) => {
  await page.route(/\/api\/projects\?limit=200$/, async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 800));
    await route.continue();
  });
  await page.goto("/validation");
  await expect(page.getByText("Loading…", { exact: true })).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route(/\/api\/projects\?limit=200$/, (route) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({
        title: "Internal Server Error",
        status: 500,
        detail: "the validation read failed",
      }),
    }),
  );
  await page.goto("/validation");
  await expect(page.locator(".problem")).toContainText("the validation read failed");
});

test("a signed-out visit is the sign-in gate", async ({ page }) => {
  await page.goto("/validation");
  await page.evaluate(() => {
    document.cookie = "conveyor_dev=; Max-Age=0; Path=/";
  });
  await page.goto("/validation");
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
});
