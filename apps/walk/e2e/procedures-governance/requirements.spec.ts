import { expect, test } from "@playwright/test";

// Requirements: the plan gate, the list, the detail with its trace links, an edit carrying
// If-Match, the empty list and a read failure.

test.describe.configure({ mode: "serial" });

const REQUIREMENT = {
  id: "00000000-0000-4000-8000-000000000701",
  kind: "requirement",
  key: "REQ-001",
  title: "Every unit carries a certificate",
  body: "A delivery unit closes only on an engine-derived certificate.",
  status: "accepted",
  version: 4,
  supersedes_id: null,
  superseded_by_id: null,
  project_id: null,
  owner_user_id: null,
  phase_code: "C",
  acceptance: {},
  links: {},
  lens: {},
  meta: {},
  created_at: "2026-11-01T09:00:00Z",
  updated_at: "2026-11-20T09:00:00Z",
  revisions: [{ seq: 4, label: "r4", status: "accepted", author_id: null }],
};
const LINK = {
  id: "00000000-0000-4000-8000-000000000702",
  from_type: "requirement",
  from_id: REQUIREMENT.id,
  to_type: "artifact",
  to_id: "00000000-0000-4000-8000-000000000703",
  kind: "satisfied_by",
  created_at: "2026-11-02T09:00:00Z",
};

function json(route: any, body: unknown) {
  return route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ output: body }) });
}

test("without requirements.list the screen is the plan gate naming it", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => entry.name !== "requirements.list");
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await page.goto("/design/requirements");
  await expect(page.getByRole("heading", { name: "Requirements is planned" })).toBeVisible();
  await expect(page.getByText(/requirements\.list is not in this role's manifest/)).toBeVisible();
});

test("the list renders each requirement and its state", async ({ page }) => {
  await page.route("**/api/actions/requirements.list/invocations", (route: any) => json(route, { items: [REQUIREMENT] }));
  await page.goto("/design/requirements");
  await expect(page.getByRole("heading", { name: "Requirements", level: 1 })).toBeVisible();
  await expect(page.getByTestId("requirements-list")).toContainText("Every unit carries a certificate");
  await expect(page.getByTestId("requirements-list")).toContainText("accepted");
  await expect(page.getByRole("button", { name: "New requirement" })).toBeVisible();
});

test("the detail renders the requirement and its trace links", async ({ page }) => {
  await page.route("**/api/actions/requirements.get/invocations", (route: any) => json(route, REQUIREMENT));
  await page.route("**/api/actions/trace_links.list/invocations", (route: any) => json(route, { items: [LINK] }));
  await page.goto(`/design/requirements/${REQUIREMENT.id}`);
  await expect(page.getByRole("heading", { name: "Every unit carries a certificate" })).toBeVisible();
  await expect(page.getByTestId("requirements-links")).toContainText("satisfied_by");
  await expect(page.getByRole("button", { name: "Transition" })).toBeVisible();
});

test("an edit carries the row's version as If-Match", async ({ page }) => {
  await page.route("**/api/actions/requirements.get/invocations", (route: any) => json(route, REQUIREMENT));
  await page.route("**/api/actions/trace_links.list/invocations", (route: any) => json(route, { items: [] }));
  let sentIfMatch: string | undefined;
  await page.route("**/api/actions/requirements.update/invocations", async (route: any) => {
    sentIfMatch = route.request().headers()["if-match"];
    await json(route, {});
  });
  await page.goto(`/design/requirements/${REQUIREMENT.id}`);
  await page.getByRole("button", { name: "Edit" }).click();
  const dialog = page.getByRole("dialog", { name: "Edit Every unit carries a certificate" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Submit" }).click();
  await expect.poll(() => sentIfMatch).toBe("4");
  await expect(dialog).toBeHidden({ timeout: 10_000 });
});

test("the empty list is the app's own message", async ({ page }) => {
  await page.route("**/api/actions/requirements.list/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/design/requirements");
  await expect(page.getByText("No requirement yet. This is the empty state, not an error.")).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route("**/api/actions/requirements.list/invocations", (route: any) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({ title: "Internal Server Error", status: 500, detail: "the requirements read failed" }),
    }),
  );
  await page.goto("/design/requirements");
  await expect(page.locator(".problem")).toContainText("the requirements read failed");
});
