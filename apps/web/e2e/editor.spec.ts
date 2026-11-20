import { expect, test } from "@playwright/test";

import { invoke } from "./helpers";

// Screen 7 — the process view's stage editor. ADR 0002 was superseded by the process model: the
// editor is an ordered list of stages, not a graph. A draft saves the whole list under If-Match;
// a published version is frozen; validation belongs to the server, not the client.
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

  // The SOP reference is a procedure row id (a UUID), not a free string: take one from the
  // seeded catalogue, so the step points at a real SOP.
  const sops = await invoke<{ items: { id: string }[] }>(request, "procedures.list", { limit: 1 });
  const sop = sops.items[0].id;

  // Stage one, through the editor's own inputs.
  await page.getByRole("button", { name: "Add step" }).click();
  await page.getByLabel("Step 1 kind").fill("agent");
  await page.getByLabel("Step 1 actor").fill("process_agent");
  await page.getByLabel("Step 1 action").fill("cards.comment");
  await page.getByLabel("Step 1 SOP").fill(sop);

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
  expect(read.steps[0].procedure_id).toBe(sop);

  await page.reload();
  await expect(page.getByLabel("Step 1 kind")).toHaveValue("agent");
  await expect(page.getByLabel("Step 1 SOP")).toHaveValue(sop);
  await expect(page.getByLabel("Step 2 kind")).toHaveValue("computed");
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("an invalid definition is refused by the server on save, in its own words", async ({
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
  // The save itself is refused by the action, in its own words — not by a client-side check.
  await page.getByRole("button", { name: "Save steps" }).click();
  await expect(page.getByRole("status")).toContainText(
    "Step 1: a computed step needs an action.",
  );

  // Nothing was written: the draft still has no steps.
  const read = await invoke<{ status: string; steps: unknown[] }>(request, "processes.get", {
    process_id: created.id,
  });
  expect(read.status).toBe("draft");
  expect(read.steps).toHaveLength(0);
});

test("a process id from nowhere is the API's refusal, not a blank page", async ({ page }) => {
  await page.goto("/build/processes/00000000-0000-7000-8000-000000000000");
  await expect(page.getByRole("heading", { name: "That did not load" })).toBeVisible();
  await expect(page.locator(".panel.problem")).toBeVisible();
});
