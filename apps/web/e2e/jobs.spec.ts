import { expect, test, type APIRequestContext } from "./fixture";

import { activateJob, createActiveBinding, createBoardCard, invoke, isDevAuth } from "./helpers";

// #563 T18-2 — Jobs: the catalogue, the release, the binding, and the server's stale-pin
// refusal. The start control sends the caller's expected hash; the 409 is the server's own.

let sequence = 0;
function unique(prefix: string): string {
  sequence += 1;
  return `${prefix} ${Date.now()}-${sequence}`;
}

async function fixture(
  request: APIRequestContext,
  title: string,
): Promise<{ jobId: string; releaseId: string; releaseHash: string; bindingId: string }> {
  const industry = await invoke<{ id: string }>(request, "industries.create", {
    key: `e2e_jobs_industry_${Date.now()}`.slice(0, 40),
    name: unique("e2e jobs industry"),
  });
  const department = await invoke<{ id: string }>(request, "departments.create", {
    name: unique("e2e jobs department"),
    industry_id: industry.id,
    ordinal: 1,
    is_primary: true,
  });
  const position = await invoke<{ id: string }>(request, "job_catalogue.create", {
    job_key: `e2e_jobs_position_${Date.now()}`.slice(0, 60),
    industry_id: industry.id,
    family: "engineering",
    title,
    definition: {
      charter: "The e2e fixture position; publish and bind its release.",
      limits: { max_iterations: 4 },
    },
    departments: [{ department_id: department.id, ordinal: 1, is_primary: true }],
  });
  // A job is born a draft (v1-w1a-4): activation opens a gate, and a second principal must
  // approve it before a release may publish or a binding may pin it.
  await activateJob(request, position.id);
  const release = await invoke<{ id: string; release_hash: string }>(
    request,
    "job_releases.publish",
    { job_id: position.id },
  );
  const agents = await invoke<{ items: { id: string }[] }>(request, "capability_agents.list", {
    page: { limit: 1 },
  });
  const agent = agents.items[0];
  expect(agent, "the dev seed carries one fixture capability agent").toBeTruthy();
  const binding = await createActiveBinding<{ id: string }>(request, {
    job_id: position.id,
    release_id: release.id,
    executor_kind: "internal_agent",
    capability_agent_id: agent!.id,
    // #833's spend-budget gate refuses an internal-agent run whose binding declares no
    // ceiling; the fixture declares the smallest honest one.
    meta: { limits: { cost_budget_usd: 1 } },
  });
  return {
    jobId: position.id,
    releaseId: release.id,
    releaseHash: release.release_hash,
    bindingId: binding.id,
  };
}

test("a stale binding refuses with the server code", async ({ page, request }) => {
  test.skip(
    !isDevAuth(),
    "activating a job needs the dev seed's second principal to approve the gate",
  );
  const title = unique("e2e job");
  const built = await fixture(request, title);
  const card = await createBoardCard(request, unique("e2e stale run"));

  await page.goto("/jobs");
  const row = page.locator('[data-testid="jobs-catalogue"] tr', { hasText: title });
  await expect(row).toBeVisible();
  await row.getByRole("button", { name: "Open" }).click();

  await expect(page.locator('[data-testid="jobs-releases"]')).toContainText(
    built.releaseHash.slice(0, 14),
  );
  await expect(page.locator('[data-testid="jobs-bindings"]')).toContainText("internal_agent");

  // The card is not assigned to the binding; the requested binding and a wrong pin are enough
  // for the server to refuse the run as stale, before it writes anything.
  await page.locator('label.field:has-text("Card id") input').fill(card.card.id);
  await page
    .locator('label.field:has-text("Expected release hash") input')
    .fill("0".repeat(64));
  await page
    .locator('[data-testid="jobs-bindings"] tr', { hasText: "internal_agent" })
    .getByRole("button", { name: "Start run" })
    .click();

  const alert = page.locator("p.notice[role=alert]");
  await expect(alert).toContainText("pins release");
});
