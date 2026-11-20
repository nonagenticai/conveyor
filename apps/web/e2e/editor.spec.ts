import { expect, test } from "@playwright/test";

import { invoke } from "./helpers";

// Screen 7 — the process view's stage editor. ADR 0002 was superseded by the process model: the
// editor is an ordered list of stages, not a graph. A draft saves the whole list under If-Match;
// a published version is frozen; validation belongs to the server, at publish.
//
// The process created here is the spec's own, so the seeded catalogue is untouched.

test("the stages render in order and a save round-trips through processes.update", async ({
  page,
  request,
}) => {
  const stamp = Date.now();
  const title = `e2e editor ${stamp}`;
  const created = await invoke<{ id: string; lock_version: number }>(
    request,
    "processes.create",
    { key: `e2e-editor-${stamp}`, title },
  );

  await page.goto(`/build/processes/${created.id}`);
  await expect(page.getByText(title).first()).toBeVisible();
  // A new draft has no steps, and the editor says so before the first Add.
  await expect(page.getByText("No steps yet. Add one below.")).toBeVisible();

  // Stage one, through the editor's own inputs.
  await page.getByRole("button", { name: "Add step" }).click();
  await page.getByLabel("Step 1 kind").fill("agent");
  await page.getByLabel("Step 1 actor").fill("process_agent");
  await page.getByLabel("Step 1 action").fill("cards.comment");
  await page.getByLabel("Step 1 SOP").fill("sop-e2e-1");

  // Stage two: the list order is the array order, so this is appended, not inserted.
  await page.getByRole("button", { name: "Add step" }).click();
  await page.getByLabel("Step 2 kind").fill("computed");
  await page.getByLabel("Step 2 actor").fill("system");
  await page.getByLabel("Step 2 action").fill("cards.update");

  await page.getByRole("button", { name: "Save steps" }).click();
  await expect(page.getByText(/Saved 2 steps at lock v\d+/)).toBeVisible();

  // The saved list is the server's, not the screen's: read it back, then reload the editor.
  const read = await invoke<{
    status: string;
    steps: { kind: string; actor: string; procedure_id: string | null }[];
  }>(request, "processes.get", { process_id: created.id });
  expect(read.status).toBe("draft");
  expect(read.steps.map((step) => step.kind)).toEqual(["agent", "computed"]);
  expect(read.steps[0].procedure_id).toBe("sop-e2e-1");

  await page.reload();
  await expect(page.getByLabel("Step 1 kind")).toHaveValue("agent");
  await expect(page.getByLabel("Step 1 SOP")).toHaveValue("sop-e2e-1");
  await expect(page.getByLabel("Step 2 kind")).toHaveValue("computed");
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("an invalid definition is refused by the server at publish, in its own words", async ({
  page,
  request,
}) => {
  const stamp = Date.now();
  const created = await invoke<{ id: string }>(request, "processes.create", {
    key: `e2e-editor-bad-${stamp}`,
    title: `e2e bad definition ${stamp}`,
  });

  await page.goto(`/build/processes/${created.id}`);
  await page.getByRole("button", { name: "Add step" }).click();
  // A computed stage must name a registered action. The draft save accepts the row — a draft is
  // allowed to be incomplete — and publish is where the rules run.
  await page.getByLabel("Step 1 kind").fill("computed");
  await page.getByLabel("Step 1 actor").fill("system");
  await page.getByRole("button", { name: "Save steps" }).click();
  await expect(page.getByText(/Saved 1 steps/)).toBeVisible();

  await page.getByRole("button", { name: "Publish" }).click();
  const refused = page.locator(".callout.bad", { hasText: "Publish refused" });
  await expect(refused).toBeVisible();
  await expect(refused).toContainText(/computed step must name a registered action/);

  // Nothing was published: the row is still a draft.
  const read = await invoke<{ status: string }>(request, "processes.get", {
    process_id: created.id,
  });
  expect(read.status).toBe("draft");
});

test("a process id from nowhere is the API's refusal, not a blank page", async ({ page }) => {
  await page.goto("/build/processes/00000000-0000-7000-8000-000000000000");
  await expect(page.getByRole("heading", { name: "That did not load" })).toBeVisible();
  await expect(page.locator(".panel.problem")).toBeVisible();
});
