import { expect, test } from "./fixture";

import { createBoardCard, invoke } from "./helpers";

// #563 T18-2 — Plan: the WBS, the baseline, the registers, and the change gate's rationale
// refusal. The refusal is the server's `422 change_rationale_required`, not a client guess.

let sequence = 0;
function unique(prefix: string): string {
  sequence += 1;
  return `${prefix} ${Date.now()}-${sequence}`;
}

test("a change needs a rationale", async ({ page, request }) => {
  const project = await createBoardCard(request, unique("e2e plan project"));
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
    .filter({ hasText: "e2e plan project" })
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
