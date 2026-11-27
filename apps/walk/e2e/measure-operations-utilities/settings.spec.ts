import { expect, test } from "@playwright/test";

// Settings: the plan gate, the four tabs (admin, inference, members, roles), a member write's
// sheet, the empty member list and a read failure.

test.describe.configure({ mode: "serial" });

const TENANT = { id: "00000000-0000-4000-8000-000000000a01", slug: "nonagentic", name: "Non-Agentic AI", settings: {} };
const MEMBER = { id: "00000000-0000-4000-8000-000000000a02", user_id: "00000000-0000-4000-8000-000000000a03", label: "alice", role: "admin", status: "active" };
const POLICY = { channel_kind: "mcp", entries: [{ action: "cards.list", allowed: true }] };

function json(route: any, body: unknown) {
  return route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ output: body }) });
}

test("without tenants.get the screen is the plan gate naming it", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => entry.name !== "tenants.get");
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await page.goto("/settings/admin");
  await expect(page.getByRole("heading", { name: "Settings is planned" })).toBeVisible();
  await expect(page.getByText(/tenants\.get is not in this role's manifest/)).toBeVisible();
});

test("the admin tab renders the tenant and its settings write", async ({ page }) => {
  await page.route("**/api/actions/tenants.get/invocations", (route: any) => json(route, TENANT));
  await page.goto("/settings/admin");
  await expect(page.getByRole("heading", { name: "Settings", level: 1 })).toBeVisible();
  await expect(page.getByTestId("settings-tenant")).toContainText("Non-Agentic AI");
  await expect(page.getByTestId("settings-tenant")).toContainText("nonagentic");
  await expect(page.getByRole("button", { name: "Update settings" })).toBeVisible();
});

test("the members tab renders each member and one write opens its sheet", async ({ page }) => {
  await page.route("**/api/actions/members.list/invocations", (route: any) => json(route, { items: [MEMBER] }));
  await page.goto("/settings/members");
  await expect(page.getByTestId("settings-members")).toContainText("alice");
  await expect(page.getByTestId("settings-members")).toContainText("admin");
  await page.getByTestId("settings-members").getByRole("button", { name: "Change role" }).click();
  const dialog = page.getByRole("dialog", { name: "Change role" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Submit" })).toBeVisible();
});

test("the roles tab renders the channel policy", async ({ page }) => {
  await page.route("**/api/actions/roles.policy_get/invocations", (route: any) => json(route, POLICY));
  await page.goto("/settings/roles");
  await expect(page.getByTestId("settings-roles")).toContainText("cards.list");
  await expect(page.getByRole("button", { name: "Set the policy" })).toBeVisible();
});

test("the inference tab renders the approval policies", async ({ page }) => {
  await page.route("**/api/actions/approval_policies.list/invocations", (route: any) => json(route, { items: [{ name: "default", kind: "process", status: "active" }] }));
  await page.goto("/settings/inference");
  await expect(page.getByTestId("settings-policies")).toContainText("default");
});

test("the empty member list is the app's own message", async ({ page }) => {
  await page.route("**/api/actions/members.list/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/settings/members");
  await expect(page.getByText("No member yet. This is the empty state, not an error.")).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route("**/api/actions/tenants.get/invocations", (route: any) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({ title: "Internal Server Error", status: 500, detail: "the tenant read failed" }),
    }),
  );
  await page.goto("/settings/admin");
  await expect(page.locator(".problem")).toContainText("the tenant read failed");
});
