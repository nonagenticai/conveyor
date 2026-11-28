import { expect, test, type APIRequestContext, type Page } from "./fixture";

import { activateJob, createActiveBinding, invoke } from "./helpers";

// T18-4 (#565) P3.2 — Improve. A kata opens only with a named instrument and a success
// threshold; without them the server refuses `422 kata_not_measurable` and the queue says why.
// The fixture creates a real active job binding through the actions (the same path jobs.spec
// uses), so the refusal is the kata's own, not a missing parent.

let sequence = 0;
function unique(prefix: string): string {
  sequence += 1;
  return `${prefix} ${Date.now()}-${sequence}`;
}

async function binding(request: APIRequestContext): Promise<string> {
  const industry = await invoke<{ id: string }>(request, "industries.create", {
    key: `e2e_improve_industry_${Date.now()}`.slice(0, 40),
    name: unique("e2e improve industry"),
  });
  const department = await invoke<{ id: string }>(request, "departments.create", {
    name: unique("e2e improve department"),
    industry_id: industry.id,
    ordinal: 1,
    is_primary: true,
  });
  const position = await invoke<{ id: string }>(request, "job_catalogue.create", {
    job_key: `e2e_improve_position_${Date.now()}`.slice(0, 60),
    industry_id: industry.id,
    family: "engineering",
    title: unique("e2e improve position"),
    definition: {
      charter: "The e2e improvement fixture position; publish and bind its release.",
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

function notice(page: Page) {
  return page.getByTestId("improve-notice");
}

test("an unmeasurable kata is not opened", async ({ page, request }) => {
  const jobBindingId = await binding(request);

  await page.goto("/improve");
  await expect(page.getByRole("heading", { name: "Improve" })).toBeVisible();

  await page.getByLabel("Job binding id").fill(jobBindingId);
  await page.getByLabel("Signature").fill(unique("e2e-unmeasurable"));
  await page.getByLabel("Kata title").fill("An unmeasurable kata");
  await page.getByLabel("Target reference").fill("process:e2e-unmeasurable");
  // Deliberately no instrument and no threshold.
  await page.getByRole("button", { name: "Open the kata" }).click();

  // The server's own sentence, naming both missing pieces; no kata view is rendered.
  await expect(notice(page)).toContainText(
    "a kata opens only with a named instrument and a success threshold",
  );
  await expect(notice(page)).toContainText("instrument_version_id");
  await expect(notice(page)).toContainText("threshold");
  await expect(page.getByTestId("kata-view")).toHaveCount(0);
});

test("the improve screen has no horizontal page scroll at 1280 and 390", async ({ page }) => {
  await page.goto("/improve");
  await expect(page.getByRole("heading", { name: "Improve" })).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `width ${width}`).toBeLessThanOrEqual(0);
  }
});
