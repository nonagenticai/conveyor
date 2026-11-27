import { expect, test, type APIRequestContext, type Page } from "./fixture";

import { activateJob, createActiveBinding, invoke } from "./helpers";

// T18-4 (#565) P1 — Tools. The grant can only narrow (a widened ceiling is the server's
// `403 tool_effect_forbidden`), and a failed probe disables the bindings that would serve new
// runs. The fixtures create a real active job binding and publish the embedded manifests
// through the actions; nothing is route-mocked.

let sequence = 0;
function unique(prefix: string): string {
  sequence += 1;
  return `${prefix} ${Date.now()}-${sequence}`;
}

type ToolkitRow = { id: string; key: string; toolkit_version: number; status: string };

async function binding(request: APIRequestContext): Promise<string> {
  const industry = await invoke<{ id: string }>(request, "industries.create", {
    key: `e2e_tools_industry_${Date.now()}`.slice(0, 40),
    name: unique("e2e tools industry"),
  });
  const department = await invoke<{ id: string }>(request, "departments.create", {
    name: unique("e2e tools department"),
    industry_id: industry.id,
    ordinal: 1,
    is_primary: true,
  });
  const position = await invoke<{ id: string }>(request, "job_catalogue.create", {
    job_key: `e2e_tools_position_${Date.now()}`.slice(0, 60),
    industry_id: industry.id,
    family: "engineering",
    title: unique("e2e tools position"),
    definition: {
      charter: "The e2e tools fixture position; publish and bind its release.",
      limits: { max_iterations: 4 },
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
  });
  return created.id;
}

/** Publish the embedded manifests, then name one published toolkit version. */
async function toolkit(request: APIRequestContext): Promise<ToolkitRow> {
  await invoke(request, "toolkits.publish", {});
  const page = await invoke<{ items: ToolkitRow[] }>(request, "toolkits.list", {
    page: { limit: 200 },
    status: "published",
  });
  const row = page.items[0];
  expect(row, "toolkits.publish makes the embedded manifests readable").toBeTruthy();
  return row!;
}

function notice(page: Page) {
  return page.getByTestId("tools-notice");
}

test("a widened grant is refused", async ({ page, request }) => {
  const jobBindingId = await binding(request);
  const published = await toolkit(request);

  await page.goto("/tools");
  await expect(page.getByRole("heading", { name: "Tools" })).toBeVisible();
  await page.getByRole("tab", { name: "Bindings" }).click();

  await page.getByLabel("Toolkit id").fill(published.id);
  await page.getByLabel("Job binding id").fill(jobBindingId);
  // Above the toolkit's strongest published effect: the grant narrows only.
  await page.getByLabel("Effect ceiling").selectOption("destructive");
  await page.getByRole("button", { name: "Save the binding" }).click();

  // The server's own code is shown; nothing is written for this grant.
  await expect(notice(page)).toContainText("tool_effect_forbidden");
  const grants = await invoke<{ items: { toolkit_id?: string }[] }>(request, "tool_bindings.list", {
    page: { limit: 200 },
    job_binding_id: jobBindingId,
  });
  expect(grants.items.some((row) => row.toolkit_id === published.id)).toBeFalsy();
});

test("a failed probe unbinds new runs", async ({ page, request }) => {
  const jobBindingId = await binding(request);
  const published = await toolkit(request);
  const created = await invoke<{ id: string }>(request, "tool_bindings.create", {
    job_binding_id: jobBindingId,
    toolkit_id: published.id,
    effect_ceiling: "read",
  });

  await page.goto("/tools");
  await expect(page.getByRole("heading", { name: "Tools" })).toBeVisible();
  await page.getByRole("tab", { name: "Toolkits & probe" }).click();

  // No CONVEYOR_EXTERNAL_<SERVICE>_URL is configured in dev: the probe is `unconfigured`,
  // which disables the bindings that would serve new runs.
  await page.getByLabel("Probe toolkit").fill(published.key);
  await page.getByRole("button", { name: "Probe", exact: true }).click();
  const probeTable = page.getByTestId("tools-probe");
  await expect(probeTable).toContainText("unconfigured");
  await expect(probeTable.locator("tbody tr").first()).toContainText("1");

  // The grant is now disabled in the ledger of grants; in-flight runs are untouched by the
  // broker (the screen says so rather than implying the run is killed).
  await page.getByRole("tab", { name: "Bindings" }).click();
  // The full id, not its visible prefix: a UUIDv7's first eight characters change only about
  // every 65 s, and another grant created in the same window would share them.
  const grant = page
    .getByTestId("tools-bindings")
    .locator(`tr[data-grant-id="${created.id}"]`);
  await expect(grant).toContainText("disabled");
  await expect(page.getByTestId("tools-bindings")).toContainText("in-flight");
});

test("the tools screen has no horizontal page scroll at 1280 and 390", async ({ page }) => {
  await page.goto("/tools");
  await expect(page.getByRole("heading", { name: "Tools" })).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `width ${width}`).toBeLessThanOrEqual(0);
  }
});
