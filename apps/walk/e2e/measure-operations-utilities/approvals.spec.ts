import { expect, test } from "@playwright/test";

// Approvals: the plan gate, the gate list with its policies, an approve carrying the approval id
// and decision, the empty list, a read failure and the signed-out gate.

test.describe.configure({ mode: "serial" });

const APPROVAL = {
  id: "00000000-0000-4000-8000-000000000901",
  gate: "procedures.publish",
  kind: "process",
  status: "pending",
  required_decisions: 1,
  requested_by_label: "conveyor session",
  created_at: "2026-11-27T10:00:00Z",
  expires_at: null,
};
const POLICY = { name: "default", kind: "process", status: "active" };

function json(route: any, body: unknown) {
  return route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ output: body }) });
}

test("without approvals.list the screen is the plan gate naming it", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => entry.name !== "approvals.list");
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await page.goto("/approvals");
  await expect(page.getByRole("heading", { name: "Approvals is planned" })).toBeVisible();
  await expect(page.getByText(/approvals\.list is not in this role's manifest/)).toBeVisible();
});

test("the gates and their policies render", async ({ page }) => {
  await page.route("**/api/actions/approvals.list/invocations", (route: any) => json(route, { items: [APPROVAL] }));
  await page.route("**/api/actions/approval_policies.list/invocations", (route: any) => json(route, { items: [POLICY] }));
  await page.goto("/approvals");
  await expect(page.getByRole("heading", { name: "Approvals", level: 1 })).toBeVisible();
  await expect(page.getByTestId("approvals-list")).toContainText("procedures.publish");
  await expect(page.getByTestId("approvals-list")).toContainText("pending");
  await expect(page.getByTestId("approvals-policies")).toContainText("default");
  await expect(page.getByRole("button", { name: "Request an approval" })).toBeVisible();
});

test("an approve carries the approval id and the decision", async ({ page }) => {
  await page.route("**/api/actions/approvals.list/invocations", (route: any) => json(route, { items: [APPROVAL] }));
  await page.route("**/api/actions/approval_policies.list/invocations", (route: any) => json(route, { items: [] }));
  let sent: any;
  await page.route("**/api/actions/approvals.decide/invocations", async (route: any) => {
    sent = route.request().postDataJSON();
    await json(route, {});
  });
  await page.goto("/approvals");
  await page.getByTestId("approvals-list").getByRole("button", { name: "Approve" }).click();
  const dialog = page.getByRole("dialog", { name: "Approve" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Submit" }).click();
  await expect.poll(() => sent?.input?.approval_id).toBe(APPROVAL.id);
  await expect.poll(() => sent?.input?.decision).toBe("approve");
});

test("the empty list is the app's own message", async ({ page }) => {
  await page.route("**/api/actions/approvals.list/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/approval_policies.list/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/approvals");
  await expect(page.getByText("No approval yet. This is the empty state, not an error.")).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route("**/api/actions/approvals.list/invocations", (route: any) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({ title: "Internal Server Error", status: 500, detail: "the approvals read failed" }),
    }),
  );
  await page.goto("/approvals");
  await expect(page.locator(".problem")).toContainText("the approvals read failed");
});
