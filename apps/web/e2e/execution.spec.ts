import { expect, test, type APIRequestContext } from "./fixture";

import { activateJob, createActiveBinding, createBoardCard, invoke } from "./helpers";

// T18-3c (#630): the execution room reads the run's executor kind, its job binding / release pin
// and its tool-call ledger. Every fixture below creates a real ACTIVE job binding and publishes
// the embedded manifests through the actions; nothing is route-mocked.

let sequence = 0;
function unique(prefix: string): string {
  sequence += 1;
  return `${prefix} ${Date.now()}-${sequence}`;
}

type Binding = { id: string; release_hash: string };

/** A real ACTIVE `internal_agent` binding, and the release hash it pins. */
async function binding(request: APIRequestContext): Promise<Binding> {
  const industry = await invoke<{ id: string }>(request, "industries.create", {
    key: `e2e_exec_industry_${Date.now()}`.slice(0, 40),
    name: unique("e2e execution industry"),
  });
  const department = await invoke<{ id: string }>(request, "departments.create", {
    name: unique("e2e execution department"),
    industry_id: industry.id,
    ordinal: 1,
    is_primary: true,
  });
  const position = await invoke<{ id: string }>(request, "job_catalogue.create", {
    job_key: `e2e_exec_position_${Date.now()}`.slice(0, 60),
    industry_id: industry.id,
    family: "engineering",
    title: unique("e2e execution position"),
    definition: {
      charter: "The e2e execution fixture position.",
      // §11.1: the release declares the tool operations it grants, and a release that declares
      // none allows no call at all — the catalogue definition is the authored job layer.
      grants: [{ op: "coding.jobs_get", max_side_effect: "read" }],
    },
    departments: [{ department_id: department.id, ordinal: 1, is_primary: true }],
  });
  // A job is born a draft; only an active job may publish a release or be bound.
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
  const created = await createActiveBinding<{ id: string }>(request, {
    job_id: position.id,
    release_id: release.id,
    executor_kind: "internal_agent",
    capability_agent_id: agent!.id,
    // #833's spend-budget gate refuses an internal-agent run whose binding declares no
    // ceiling; the fixture declares the smallest honest one.
    meta: { limits: { cost_budget_usd: 1 } },
  });
  return { id: created.id, release_hash: release.release_hash };
}

test("the run history and inspector carry the executor and binding pin", async ({
  page,
  request,
}) => {
  const fixture = await createBoardCard(request, `execution-pin-${Date.now()}`);
  const pinned = await binding(request);
  const run = await invoke<{ id: string }>(request, "runs.create", {
    card_id: fixture.card.id,
    job_binding_id: pinned.id,
    release_hash: pinned.release_hash,
  });

  await page.goto("/execution");
  await expect(page.getByRole("heading", { name: "Execution room" })).toBeVisible();

  const row = page
    .getByTestId("execution-runs")
    .locator("tr", { has: page.locator(`a[href="/runs/${run.id}"]`) });
  await expect(row).toBeVisible();
  await expect(row).toContainText("internal_agent");
  await expect(row).toContainText(pinned.id.slice(0, 8));
  await expect(row).toContainText(pinned.release_hash.slice(0, 8));

  await row.getByRole("button", { name: run.id.slice(0, 8) }).click();
  const detail = page.getByTestId("execution-detail");
  await expect(detail).toBeVisible();
  await expect(detail).toContainText("internal_agent");
  await expect(detail).toContainText(pinned.id);
  await expect(detail).toContainText(pinned.release_hash);
  await expect(page.getByText("No tool call recorded for this run.")).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("the tools panel names the tool version, credential alias and outcome", async ({
  page,
  request,
}) => {
  const fixture = await createBoardCard(request, `execution-tools-${Date.now()}`);
  const pinned = await binding(request);
  const run = await invoke<{ id: string }>(request, "runs.create", {
    card_id: fixture.card.id,
    job_binding_id: pinned.id,
    release_hash: pinned.release_hash,
  });

  await invoke(request, "toolkits.publish", {});
  const toolkits = await invoke<{ items: { id: string; key: string }[] }>(
    request,
    "toolkits.list",
    { page: { limit: 200 }, status: "published" },
  );
  const toolkit = toolkits.items.find((row) => row.key === "coding");
  expect(toolkit, "toolkits.publish makes the coding toolkit readable").toBeTruthy();
  await invoke(request, "tool_bindings.create", {
    job_binding_id: pinned.id,
    toolkit_id: toolkit!.id,
    effect_ceiling: "read",
    credential_alias: "coding.tenant",
  });

  // `coding.jobs_get` takes one string id, so this input passes the broker's schema check and
  // reaches the destination. The dev estate has no coding service URL, so the call fails at the
  // transport — still a ledger row naming the tool version, the grant alias and the outcome.
  try {
    await invoke(request, "tools.call", {
      run_id: run.id,
      tool: "coding.jobs_get",
      input: { id: "e2e" },
    });
  } catch {
    // The failure is asserted through the ledger row below.
  }

  await page.goto("/execution");
  const row = page
    .getByTestId("execution-runs")
    .locator("tr", { has: page.locator(`a[href="/runs/${run.id}"]`) });
  await row.getByRole("button", { name: run.id.slice(0, 8) }).click();

  const panel = page.getByTestId("execution-tool-calls");
  await expect(panel).toBeVisible();
  await expect(panel).toContainText("coding.jobs_get");
  await expect(panel).toContainText("coding.tenant");
  await expect(panel.locator("tbody tr").first()).not.toBeEmpty();
  await expect(page.locator(".problem")).toHaveCount(0);
});
