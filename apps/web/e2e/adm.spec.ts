import { expect, test, type APIRequestContext } from "./fixture";

import { isDevAuth, invoke } from "./helpers";

// #566 T18-5 (W5) — the ADM capability is the real T9 screen at its measured destination
// (`/design/adm` in SPEC_V2 §10.5), not a "not yet live" gate. The fixtures go through the real
// actions; the phase refusal is the server's own `architecture_artifact_missing` /
// `architecture_contract_not_live`, shown verbatim, never a client guess.

let sequence = 0;
function unique(prefix: string): string {
  sequence += 1;
  return `${prefix} ${Date.now()}-${sequence}`;
}

type ProjectRow = { id: string; key: string; name: string };

async function project(request: APIRequestContext, title: string): Promise<ProjectRow> {
  return invoke<ProjectRow>(request, "projects.create", {
    title,
    project_type_code: "delivery",
  });
}

test("the ADM destination renders the capability, its standard and the Board", async ({
  page,
  request,
}) => {
  // A tailored profile is what makes the phase gate real; tailoring also seeds the artefact
  // standard the Capability tab renders.
  const profileName = unique("e2e adm profile");
  await invoke(request, "architecture.profile_tailor", {
    key: `e2e_adm_${Date.now()}`,
    name: profileName,
    artifact_requirements: { a: { hld: ["catalog"] } },
    contract_kinds: { a: "delivery_contract" },
  });

  await page.goto("/design/adm");
  await expect(page.locator('[data-testid="adm-screen"]')).toBeVisible();
  await expect(page.locator('[data-testid="adm-profile"]')).toContainText(profileName);
  // The standard the profile was tailored against is rendered beside it.
  await expect(page.locator('[data-testid="adm-profile"]')).toContainText("catalog");

  // Opening the Board is a real action; the panel re-reads and renders the row. The tenant's
  // Board is a singleton, so a rerun against the same database finds it already open.
  const open = page.getByRole("button", { name: "Open the Board" });
  if (await open.count()) {
    await open.click();
  }
  await expect(page.locator('[data-testid="adm-board"]')).toContainText("Architecture Board");
});

test("a phase cannot close without its mandatory artefact and a live contract", async ({
  page,
  request,
}) => {
  const title = unique("e2e adm phase project");
  const row = await project(request, title);
  await invoke(request, "architecture.profile_tailor", {
    key: `e2e_adm_gate_${Date.now()}`,
    name: unique("e2e adm gate profile"),
    artifact_requirements: { a: { hld: ["catalog"] } },
    contract_kinds: { a: "delivery_contract" },
  });
  await invoke(request, "wbs.create", {
    project_id: row.id,
    kind: "phase",
    title: "Phase A — Architecture vision",
    position: 2,
    meta: { phase_code: "a" },
  });

  await page.goto("/design/adm");
  await page.getByRole("button", { name: "ADM phases" }).click();
  await page
    .locator('label.filter:has-text("Architecture project") select')
    .selectOption({ label: `${row.key} · ${row.name}` });

  await expect(page.locator('[data-testid="adm-phases"]')).toContainText(
    "Phase A — Architecture vision",
  );
  // Scope to the phase panel and match the button exactly: the shell's "Advanced" toggle also
  // contains the substring "Advance".
  await page
    .locator('[data-testid="adm-phases"]')
    .getByRole("button", { name: "Advance", exact: true })
    .click();
  await expect(page.locator('[data-testid="adm-notice"]')).toContainText("Refused");
});

test("an empty tenant gets the honest empty or permission state, never a fixture", async ({
  page,
  context,
  baseURL,
}) => {
  test.skip(!isDevAuth(), "the deployed suite has one principal; the empty tenant is a dev seed");
  const host = new URL(baseURL!).hostname;
  await context.addCookies([{ name: "conveyor_dev", value: "bob", domain: host, path: "/" }]);

  await page.goto("/design/adm");
  await expect(page.locator('[data-testid="adm-screen"]')).toBeVisible();
  // The empty tenant either lacks the read (the server's forbidden panel) or holds no profile
  // (the screen's own empty state); both are honest, neither is a fixture.
  await expect(page.locator('[data-testid="adm-profile"]')).toContainText(
    /no profile|forbidden|permission|allowed/i,
  );
});
