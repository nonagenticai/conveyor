import { expect, test, type APIRequestContext } from "./fixture";

import { authFor, createBoardCard, invoke } from "./helpers";

// #563 T18-2 — Organisation and Jobs: the tenant's positions, the catalogue and release, the
// binding, and the server's own refusals. The fixtures go through the real actions; no screen
// renders a fixture.

let sequence = 0;
function unique(prefix: string): string {
  sequence += 1;
  return `${prefix} ${Date.now()}-${sequence}`;
}

async function industry(
  request: APIRequestContext,
  key: string,
  name: string,
): Promise<string> {
  const row = await invoke<{ id: string }>(request, "industries.create", { key, name });
  return row.id;
}

async function department(
  request: APIRequestContext,
  name: string,
  industryId: string,
): Promise<string> {
  const row = await invoke<{ id: string }>(request, "departments.create", {
    name,
    industry_id: industryId,
    ordinal: 1,
    is_primary: true,
  });
  return row.id;
}

async function position(
  request: APIRequestContext,
  title: string,
  industryId: string,
  departmentId: string,
): Promise<string> {
  const row = await invoke<{ id: string }>(request, "job_catalogue.create", {
    job_key: `e2e_position_${Date.now()}`.slice(0, 60),
    industry_id: industryId,
    family: "engineering",
    title,
    departments: [{ department_id: departmentId, ordinal: 1, is_primary: true }],
  });
  return row.id;
}

test("the organisation renders the tenant's positions", async ({ page, request }) => {
  const industryName = unique("e2e industry");
  const industryId = await industry(request, `e2e_industry_${Date.now()}`, industryName);
  const departmentName = unique("e2e department");
  const departmentId = await department(request, departmentName, industryId);
  const title = unique("e2e position");
  await position(request, title, industryId, departmentId);

  await page.goto("/organisation");

  await expect(page.locator('[data-testid="organisation-industries"]')).toContainText(industryName);
  await expect(page.locator('[data-testid="organisation-departments"]')).toContainText(
    departmentName,
  );
  await expect(page.locator('[data-testid="organisation-jobs"]')).toContainText(title);
});

test("the organisation writes through its actions", async ({ page, request }) => {
  const industryName = unique("e2e form industry");
  await industry(request, `e2e_form_industry_${Date.now()}`, industryName);
  await invoke(request, "approval_policies.set", {
    action: "cards.archive",
    approval: "required",
    self_approval: "forbidden",
  });
  const departmentName = unique("e2e form department");

  await page.goto("/organisation");
  await page
    .locator('label.field:has-text("Name") input')
    .first()
    .fill(departmentName);
  await page
    .locator('label.field:has-text("Industry") select')
    .first()
    .selectOption({ label: industryName });
  await page.getByRole("button", { name: "Create department" }).click();

  await expect(page.locator("p.notice[role=alert]")).toContainText("Department created.");
  await expect(page.locator('[data-testid="organisation-departments"]')).toContainText(
    departmentName,
  );
  await expect(page.locator('[data-testid="organisation-approval-policies"]')).toContainText(
    "cards.archive",
  );
});

test("the organisation group's other destinations render", async ({ page }) => {
  for (const [path, heading] of [
    ["/agents", "Agents"],
    ["/decisions", "Decisions"],
    ["/project-types", "Project types"],
  ] as const) {
    await page.goto(path);
    await expect(page.getByRole("heading", { name: heading })).toBeVisible();
    await expect(page.locator(".problem[role=alert]")).toHaveCount(0);
  }
});

test("a self-approval is refused", async ({ page, request }) => {
  await invoke(request, "approval_policies.set", {
    action: "cards.archive",
    approval: "required",
    self_approval: "forbidden",
  });
  const title = unique("e2e self-gate");
  const fixture = await createBoardCard(request, title);

  await page.goto(`/board/${fixture.boardId}`);
  const card = page.locator(".card", { hasText: title });
  await expect(card).toBeVisible();
  await card.getByRole("button", { name: "Archive" }).click();
  await expect(page.locator(".toast")).toContainText("Waiting for approval");

  const target = `card/${fixture.card.id}/archive`;
  const gates = await invoke<{ items: { id: string; target?: string }[] }>(
    request,
    "approvals.list",
    { status: "pending", limit: 50 },
  );
  const gate = gates.items.find((row) => row.target === target);
  expect(gate, "the archive left a pending gate naming its target").toBeTruthy();

  // The screen names the missing second approver rather than hiding the control.
  await page.goto("/approvals");
  const row = page.locator("article").filter({ hasText: target }).first();
  await expect(row).toBeVisible();
  await expect(row.locator("button", { hasText: "Approve" })).toBeDisabled();
  await expect(row).toContainText(/self|approv/i);

  // And the server refuses the same act if the caller bypasses the control.
  const refusal = await request.post(`/api/actions/approvals.decide/invocations`, {
    headers: authFor("alice"),
    data: { input: { approval_id: gate!.id, decision: "approve" } },
  });
  expect(refusal.status()).toBeGreaterThanOrEqual(400);
  const body = await refusal.text();
  expect(body).toMatch(/self|approv/i);
});
