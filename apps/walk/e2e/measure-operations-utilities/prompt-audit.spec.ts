import { expect, test } from "@playwright/test";

// Prompt audit: the plan gate, the audits and findings, the start write's sheet, the empty list
// and a read failure.

test.describe.configure({ mode: "serial" });

const AUDIT = { id: "00000000-0000-4000-8000-000000000e01", name: "audit-1", title: "audit-1", status: "succeeded", prompt_ref: "cards.create", created_at: "2026-11-27T10:00:00Z" };
const FINDING = { id: "00000000-0000-4000-8000-000000000e02", name: "finding-1", title: "finding-1", status: "open", severity: "high" };

function json(route: any, body: unknown) {
  return route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ output: body }) });
}

test("without prompt_audits.list the screen is the plan gate naming it", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => entry.name !== "prompt_audits.list");
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await page.goto("/measure/prompt-audit");
  await expect(page.getByRole("heading", { name: "Prompt audit is planned" })).toBeVisible();
  await expect(page.getByText(/prompt_audits\.list is not in this role's manifest/)).toBeVisible();
});

test("the audits and findings render", async ({ page }) => {
  await page.route("**/api/actions/prompt_audits.list/invocations", (route: any) => json(route, { items: [AUDIT] }));
  await page.route("**/api/actions/prompt_findings.list/invocations", (route: any) => json(route, { items: [FINDING] }));
  await page.goto("/measure/prompt-audit");
  await expect(page.getByRole("heading", { name: "Prompt audit", level: 1 })).toBeVisible();
  await expect(page.getByTestId("prompt-audits")).toContainText("audit-1");
  await expect(page.getByTestId("prompt-findings")).toContainText("finding-1");
  await expect(page.getByRole("button", { name: "Start an audit" })).toBeVisible();
});

test("the start write opens its sheet", async ({ page }) => {
  await page.route("**/api/actions/prompt_audits.list/invocations", (route: any) => json(route, { items: [AUDIT] }));
  await page.route("**/api/actions/prompt_findings.list/invocations", (route: any) => json(route, { items: [FINDING] }));
  await page.goto("/measure/prompt-audit");
  await page.getByRole("button", { name: "Start an audit" }).click();
  const dialog = page.getByRole("dialog", { name: "Start an audit" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Submit" })).toBeVisible();
});

test("the empty audits is the app's own message", async ({ page }) => {
  await page.route("**/api/actions/prompt_audits.list/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/prompt_findings.list/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/measure/prompt-audit");
  await expect(page.getByText("No audit has run in this tenant yet.")).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route("**/api/actions/prompt_audits.list/invocations", (route: any) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({ title: "Internal Server Error", status: 500, detail: "the audits read failed" }),
    }),
  );
  await page.goto("/measure/prompt-audit");
  await expect(page.locator(".problem")).toContainText("the audits read failed");
});
