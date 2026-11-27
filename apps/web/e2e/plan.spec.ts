import { expect, test, type Page } from "./fixture";

import { createBoardCard, invoke, isDevAuth } from "./helpers";

const baseURL = process.env.BASE_URL ?? "http://127.0.0.1:8931";

/** Swap the dev cookie. The file's storageState is alice (admin of dev-a). */
async function as(page: Page, login: string) {
  const host = new URL(baseURL).hostname;
  await page.context().addCookies([
    { name: "conveyor_dev", value: login, domain: host, path: "/" },
  ]);
}

// #563 T18-2 — Plan: the WBS, the baseline, the registers, and the change gate's rationale
// refusal. The refusal is the server's `422 change_rationale_required`, not a client guess.

let sequence = 0;
function unique(prefix: string): string {
  sequence += 1;
  return `${prefix} ${Date.now()}-${sequence}`;
}

test("a change needs a rationale", async ({ page, request }) => {
  // The project name must be the FILTER, not just the fixture's name: this spec creates one
  // project per run, the table sorts by key, and a prefix filter's `.first()` would open an
  // older run's project and assert its empty WBS. (Measured on a database with two runs.)
  const projectTitle = unique("e2e plan project");
  const project = await createBoardCard(request, projectTitle);
  const wbsTitle = unique("e2e wbs");
  await invoke(request, "wbs.create", {
    project_id: project.projectId,
    kind: "deliverable",
    title: wbsTitle,
  });
  const registerTitle = unique("e2e register");
  await invoke(request, "registers.create", {
    project_id: project.projectId,
    kind: "risk",
    title: registerTitle,
  });
  await invoke(request, "baselines.take", { project_id: project.projectId });

  await page.goto("/plan");
  const row = page
    .locator('[data-testid="plan-projects"] tr')
    .filter({ hasText: projectTitle })
    .first();
  await expect(row).toBeVisible();
  await row.getByRole("button", { name: "Open" }).click();

  await expect(page.locator('[data-testid="plan-wbs"]')).toContainText(wbsTitle);
  await expect(page.locator('[data-testid="plan-baselines"]')).toContainText("scope");
  await expect(page.locator('[data-testid="plan-registers"]')).toContainText(registerTitle);

  // A change with a baseline and a binding but no rationale is refused by the server. The WBS
  // and register forms carry their own Title/Baseline fields, so scope every fill to the change
  // panel that owns the Request change button.
  const changeForm = page.locator(".panel").filter({
    has: page.getByRole("button", { name: "Request change" }),
  });
  await changeForm.locator('label.field:has-text("Baseline") select').selectOption({ index: 1 });
  await changeForm
    .locator('label.field:has-text("PM job binding") input')
    .fill("00000000-0000-4000-8000-000000000001");
  await changeForm.locator('label.field:has-text("Title") input').fill(unique("e2e change"));
  await changeForm.locator('label.field:has-text("Impact") input').fill("1");
  await changeForm.getByRole("button", { name: "Request change" }).click();

  const notice = page.locator("p.notice[role=alert]");
  await expect(notice).toContainText("rationale");
});

// #848 — the empty project list before the populated path, and the read's error panel.
// `fr-test`'s kata/execution tenants hold no project and stay that way for the whole suite —
// not dev-b, which processes.spec.ts:138 fills with a board card.
test("an empty tenant gets the empty project list", async ({ page }) => {
  test.skip(!isDevAuth(), "the deployed suite has one principal; fr-test is a dev seed");
  await as(page, "fr-test");
  await page.goto("/plan");
  await expect(page.getByRole("heading", { name: "Plan", exact: true })).toBeVisible();
  await expect(page.getByText("No project in this tenant yet.")).toBeVisible();
  await expect(page.locator('[data-testid="plan-projects"]')).toHaveCount(0);
});

test("a failed project read is the error panel, with a retry", async ({ page }) => {
  await page.route(/\/api\/projects\?limit=200/, (route) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({ detail: "forced by the spec" }),
    }),
  );
  await page.goto("/plan");
  const panel = page.locator("div.panel.problem[role=alert]");
  await expect(panel.getByRole("heading", { name: "That did not load" })).toBeVisible();
  await expect(panel).toContainText("forced by the spec");
  await expect(panel.getByRole("button", { name: "Try again" })).toBeVisible();
});
