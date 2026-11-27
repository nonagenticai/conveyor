import { expect, test } from "@playwright/test";

// Programme: the definition read, the T10 gates (absent manifest and the always-on benefit
// gate), and the empty/loading/error/gate states.

test.describe.configure({ mode: "serial" });

const PROGRAMME = {
  id: "00000000-0000-4000-8000-000000000030",
  name: "S1 programme",
  status: "active",
  owner_user_id: "00000000-0000-4000-8000-000000000031",
  version: 4,
};

function json(route: any, body: unknown) {
  return route.fulfill({
    status: 200,
    contentType: "application/json",
    body: JSON.stringify({ output: body }),
  });
}

test("the programme renders with its project count and the T10 benefit gate", async ({ page }) => {
  await page.route("**/api/actions/programmes.list/invocations", (route) =>
    json(route, { items: [PROGRAMME] }),
  );
  await page.route(/\/api\/projects\?limit=200$/, (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ items: [{ id: "p1", programme_id: PROGRAMME.id }] }),
    }),
  );
  await page.goto("/programme");
  await expect(page.getByRole("heading", { name: "Programme", level: 1 })).toBeVisible();
  const row = page.getByRole("row", { name: /S1 programme/ });
  await expect(row).toContainText("1");
  await expect(
    page.getByText("The benefit profiles, the benefits map and the tranche reviews are not rendered here yet"),
  ).toBeVisible();
});

test("without programmes.list the screen is the T10 gate naming it", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => entry.name !== "programmes.list");
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await page.goto("/programme");
  await expect(page.getByRole("heading", { name: "Not yet live — T10" })).toBeVisible();
  await expect(
    page.getByText("The programme definition read is not in this principal's manifest"),
  ).toBeVisible();
});

test("the empty programme list is the app's own message", async ({ page }) => {
  await page.route("**/api/actions/programmes.list/invocations", (route) =>
    json(route, { items: [] }),
  );
  await page.goto("/programme");
  await expect(page.getByText("No programmes in this tenant yet.", { exact: true })).toBeVisible();
});

test("the loading state is shown while the read is in flight", async ({ page }) => {
  await page.route("**/api/actions/programmes.list/invocations", async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 800));
    await route.continue();
  });
  await page.goto("/programme");
  await expect(page.getByText("Loading…", { exact: true })).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route("**/api/actions/programmes.list/invocations", (route) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({
        title: "Internal Server Error",
        status: 500,
        detail: "the programme read failed",
      }),
    }),
  );
  await page.goto("/programme");
  await expect(page.locator(".problem")).toContainText("the programme read failed");
});

test("a signed-out visit is the sign-in gate", async ({ page }) => {
  await page.goto("/programme");
  await page.evaluate(() => {
    document.cookie = "conveyor_dev=; Max-Age=0; Path=/";
  });
  await page.goto("/programme");
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
});
