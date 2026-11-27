import { expect, test } from "@playwright/test";

// Authoring: the plan gate, the intakes and their drafted assets, the Draft write carrying the
// intake id it was opened from, the empty state and a read failure.

test.describe.configure({ mode: "serial" });

const INTAKE = {
  id: "00000000-0000-4000-8000-000000000401",
  brief: "Convert the onboarding SOP",
  sources: [],
  status: "open",
  version: 1,
  created_at: "2026-11-27T10:00:00Z",
};
const ASSET = {
  id: "00000000-0000-4000-8000-000000000402",
  intake_id: INTAKE.id,
  kind: "procedure",
  key: "onboarding-sop",
  payload: {},
  depends_on: [],
  status: "approved",
  review_approval_id: null,
  published_at: null,
  version: 1,
};

function json(route: any, body: unknown) {
  return route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ output: body }) });
}

test("without authoring.intakes the screen is the plan gate naming it", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => entry.name !== "authoring.intakes");
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await page.goto("/authoring");
  await expect(page.getByRole("heading", { name: "Authoring is planned" })).toBeVisible();
  await expect(page.getByText(/authoring\.intakes is not in this role's manifest/)).toBeVisible();
});

test("the intakes and their assets render", async ({ page }) => {
  await page.route("**/api/actions/authoring.intakes/invocations", (route: any) => json(route, { items: [INTAKE] }));
  await page.route("**/api/actions/authoring.assets/invocations", (route: any) => json(route, { items: [ASSET] }));
  await page.goto("/authoring");
  await expect(page.getByRole("heading", { name: "Authoring", level: 1 })).toBeVisible();
  await expect(page.getByTestId("authoring-intakes")).toContainText("Convert the onboarding SOP");
  await expect(page.getByTestId("authoring-assets")).toContainText("onboarding-sop");
  await expect(page.getByTestId("authoring-assets")).toContainText("approved");
  await expect(page.getByRole("button", { name: "New intake" })).toBeVisible();
});

test("the Publish write carries the asset id it was opened from", async ({ page }) => {
  await page.route("**/api/actions/authoring.intakes/invocations", (route: any) => json(route, { items: [INTAKE] }));
  await page.route("**/api/actions/authoring.assets/invocations", (route: any) => json(route, { items: [ASSET] }));
  let sent: any;
  await page.route("**/api/actions/authoring.publish/invocations", async (route: any) => {
    sent = route.request().postDataJSON();
    await json(route, {});
  });
  await page.goto("/authoring");
  await page.getByTestId("authoring-assets").getByRole("button", { name: "Publish" }).click();
  const dialog = page.getByRole("dialog", { name: "Publish" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Submit" }).click();
  await expect.poll(() => sent?.input?.asset_id).toBe(ASSET.id);
  await expect(dialog).toBeHidden({ timeout: 10_000 });
});

test("the empty intake list is the app's own message", async ({ page }) => {
  await page.route("**/api/actions/authoring.intakes/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/authoring.assets/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/authoring");
  await expect(page.getByText("No intake yet. This is the empty state, not an error.")).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route("**/api/actions/authoring.intakes/invocations", (route: any) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({ title: "Internal Server Error", status: 500, detail: "the intake read failed" }),
    }),
  );
  await page.goto("/authoring");
  await expect(page.locator(".problem")).toContainText("the intake read failed");
});
