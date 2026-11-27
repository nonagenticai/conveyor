import { expect, test, type APIRequestContext } from "./fixture";

import { createBoardCard, invoke } from "./helpers";

// #566 T18-5 (W5) — Knowledge design, sources & retrieval, and the memory/lesson workbench. The
// fixtures go through the real actions; the refusals are the server's own codes, shown verbatim.

let sequence = 0;
function unique(prefix: string): string {
  sequence += 1;
  return `${prefix} ${Date.now()}-${sequence}`;
}

type JobRow = { id: string; key: string; title: string };

async function position(
  request: APIRequestContext,
  title: string,
  industryId: string,
  departmentId: string,
  modelRef: string,
): Promise<JobRow> {
  return invoke<JobRow>(request, "job_catalogue.create", {
    job_key: `e2e_knowledge_${Date.now()}_${sequence}`.slice(0, 60),
    industry_id: industryId,
    family: "engineering",
    title,
    definition: {
      charter: "The knowledge-design fixture position.",
      model_ref: modelRef,
      knowledge: "base → software → delivery",
      memory: "work-package hot + one digest",
      executor_kind: "internal_agent",
    },
    departments: [{ department_id: departmentId, ordinal: 1, is_primary: true }],
  });
}

test("knowledge design shows the effective configuration and the department conflict", async ({
  page,
  request,
}) => {
  const industry = await invoke<{ id: string }>(request, "industries.create", {
    key: `e2e_knowledge_industry_${Date.now()}`,
    name: unique("e2e knowledge industry"),
  });
  const department = await invoke<{ id: string; name: string }>(request, "departments.create", {
    name: unique("e2e knowledge department"),
    industry_id: industry.id,
    ordinal: 1,
    is_primary: true,
  });
  const small = await position(
    request,
    unique("e2e knowledge small"),
    industry.id,
    department.id,
    "tier-small",
  );
  const large = await position(
    request,
    unique("e2e knowledge large"),
    industry.id,
    department.id,
    "tier-large",
  );

  await page.goto("/knowledge");
  await page
    .locator('label.filter:has-text("Position") select')
    .selectOption({ label: `${small.key} · ${small.title}` });

  await expect(page.locator('[data-testid="knowledge-effective"]')).toContainText("tier-small");
  await expect(page.locator('[data-testid="knowledge-departments"]')).toContainText(department.name);
  const conflicts = page.locator('[data-testid="knowledge-conflicts"]');
  await expect(conflicts).toContainText(large.title);
  await expect(conflicts).toContainText("model_ref");
});

test("sources & retrieval asks the packs and shows the server's refusal", async ({ page }) => {
  await page.goto("/knowledge/sources");
  await expect(page.locator('[data-testid="sources-retrieval"]')).toBeVisible();
  // The source catalogue is the real authoring.assets read; an empty tenant says what creates one.
  await expect(page.locator('[data-testid="sources-catalogue"]')).toBeVisible();

  // An empty capability is refused by the server's input validation, and the screen shows it.
  await page.getByRole("button", { name: "Ask the packs" }).click();
  await expect(page.locator(".problem[role=alert]")).toContainText(/invalid|capability|uuid|input/i);
});

test("memory recalls a work package and refuses a lesson with one evidence ref", async ({
  page,
  request,
}) => {
  const fixture = await createBoardCard(request, unique("e2e memory card"));

  await page.goto("/knowledge/memory");
  await expect(page.locator('[data-testid="memory-workbench"]')).toBeVisible();
  await page.locator('label.field:has-text("Card id") input').fill(fixture.card.id);
  await page.getByRole("button", { name: "Recall" }).click();
  await expect(page.locator('[data-testid="memory-counts"]')).toContainText("0 hot row");

  // One evidence reference is the server's `422 lesson_evidence_insufficient`, never a stored
  // lesson: the notice carries the refusal and the lifecycle table stays absent.
  await page.locator('label.field:has-text("Statement") input').fill(unique("e2e one-ref lesson"));
  await page.locator('label.field:has-text("Evidence 1") input').fill(fixture.card.id);
  await page.getByRole("button", { name: "Propose" }).click();
  await expect(page.locator('[data-testid="memory-notice"]')).toContainText("Refused");
});

test("measure renders the retained instruments", async ({ page }) => {
  await page.goto("/measure");
  await expect(page.locator('[data-testid="measure"]')).toBeVisible();
  for (const id of ["measure-usage", "measure-inference", "measure-prompt-audit", "measure-benchmarks", "measure-evals"]) {
    await expect(page.locator(`[data-testid="${id}"]`)).toBeVisible();
  }
  await expect(page.locator(".problem[role=alert]")).toHaveCount(0);
});
