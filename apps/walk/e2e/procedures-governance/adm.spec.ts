import { expect, test } from "@playwright/test";

// ADM: the plan gate, the adopted profile and its standard, the Board, the contracts and the
// artefacts, the acts as sheet opens, the empty repository and a read failure.

test.describe.configure({ mode: "serial" });

const PROFILE = {
  id: "00000000-0000-4000-8000-000000000501",
  key: "togaf-adm",
  name: "TOGAF ADM with the estate's artefacts",
  phase_set: {},
  artifact_requirements: {},
  contract_kinds: {},
  status: "adopted",
  adopted_at: "2026-11-01T09:00:00Z",
  board_decision_id: null,
  version: 2,
};
const BOARD = { id: "00000000-0000-4000-8000-000000000502", project_id: "00000000-0000-4000-8000-000000000503", name: "Architecture Board", version: 1 };
const CONTRACT = {
  id: "00000000-0000-4000-8000-000000000504",
  project_id: "00000000-0000-4000-8000-000000000503",
  kind: "architecture",
  phase_code: "C",
  title: "The delivery contract for the walk",
  body: {},
  status: "approved",
  approval_id: null,
  approved_by: "00000000-0000-4000-8000-000000000505",
  approved_at: "2026-11-02T09:00:00Z",
  requested_by: null,
  requester_unknown: false,
  supersedes_id: null,
};
const ARTIFACT = {
  id: "00000000-0000-4000-8000-000000000506",
  project_id: null,
  artifact_type_id: null,
  phase_code: "B",
  plateau: "capability",
  artifact_kind: "catalog",
  title: "The application catalogue",
  status: "approved",
  artifact_version: 3,
  author_id: null,
  reviewer_id: null,
  reviewed_at: null,
  change_initiative: "",
};

function json(route: any, body: unknown) {
  return route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ output: body }) });
}

test("without architecture.profile_get the screen is the plan gate naming it", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => entry.name !== "architecture.profile_get");
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await page.goto("/design/adm");
  await expect(page.getByRole("heading", { name: "ADM is planned" })).toBeVisible();
  await expect(page.getByText(/architecture\.profile_get is not in this role's manifest/)).toBeVisible();
});

test("the repository renders: profile, Board, contracts and artefacts", async ({ page }) => {
  await page.route("**/api/actions/architecture.profile_get/invocations", (route: any) => json(route, { profile: PROFILE, standard: [] }));
  await page.route("**/api/actions/architecture.board_list/invocations", (route: any) => json(route, { items: [BOARD] }));
  await page.route("**/api/actions/architecture.contracts_list/invocations", (route: any) => json(route, { items: [CONTRACT] }));
  await page.route("**/api/actions/architecture.artifacts_list/invocations", (route: any) => json(route, { items: [ARTIFACT] }));
  await page.goto("/design/adm");
  await expect(page.getByRole("heading", { name: "ADM", level: 1 })).toBeVisible();
  await expect(page.getByTestId("adm-profile")).toContainText("TOGAF ADM");
  await expect(page.getByTestId("adm-board")).toContainText("Architecture Board");
  await expect(page.getByTestId("adm-contracts")).toContainText("The delivery contract for the walk");
  await expect(page.getByTestId("adm-artifacts")).toContainText("The application catalogue");
  await expect(page.getByRole("button", { name: "Advance a phase" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Request a contract" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Open the Board" })).toBeVisible();
});

test("an artefact write opens its sheet", async ({ page }) => {
  await page.route("**/api/actions/architecture.profile_get/invocations", (route: any) => json(route, { profile: PROFILE, standard: [] }));
  await page.route("**/api/actions/architecture.board_list/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/architecture.contracts_list/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/architecture.artifacts_list/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/design/adm");
  await page.getByRole("button", { name: "New artefact" }).click();
  const dialog = page.getByRole("dialog", { name: "New artefact" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Submit" })).toBeVisible();
});

test("the empty repository is the app's own message", async ({ page }) => {
  await page.route("**/api/actions/architecture.profile_get/invocations", (route: any) => json(route, { profile: null, standard: [] }));
  await page.route("**/api/actions/architecture.board_list/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/architecture.contracts_list/invocations", (route: any) => json(route, { items: [] }));
  await page.route("**/api/actions/architecture.artifacts_list/invocations", (route: any) => json(route, { items: [] }));
  await page.goto("/design/adm");
  await expect(page.getByText("No Board decision yet.")).toBeVisible();
  await expect(page.getByText(/No contract yet\. A project requests one/)).toBeVisible();
  await expect(page.getByText(/No artefact yet\. Each phase produces/)).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route("**/api/actions/architecture.profile_get/invocations", (route: any) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({ title: "Internal Server Error", status: 500, detail: "the profile read failed" }),
    }),
  );
  await page.goto("/design/adm");
  await expect(page.locator(".problem")).toContainText("the profile read failed");
});
