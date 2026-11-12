import { expect, test, type Page } from "@playwright/test";

import { createBoardCard, invoke } from "./helpers";

const baseURL = process.env.BASE_URL ?? "http://127.0.0.1:8931";

/** Swap the dev cookie. The file's storageState is alice; bob is the empty tenant. */
async function as(page: Page, login: string) {
  const host = new URL(baseURL).hostname;
  await page.context().addCookies([
    { name: "conveyor_dev", value: login, domain: host, path: "/" },
  ]);
}

type StepView = {
  position: number;
  kind: string;
  actor: string;
  action: string;
  procedure_id?: string;
};

type ProcessView = {
  id: string;
  key: string;
  title: string;
  status: string;
  version: number;
  lock_version: number;
  steps: StepView[];
};

test("a process is authored, refuses a bad publish, is fixed, published and run", async ({
  page,
  request,
}) => {
  // dev-b: a real tenant with the catalogue but no process catalogue. Its runs resolve no
  // capability, so the pre-run readiness guard has nothing to refuse — the happy path a person
  // gets before any capability is bound. (dev-a's 123 seeded process agents trip the v1
  // "exactly one active executor" rule at the door; that is reported as a backend blocker.)
  await as(page, "bob");
  await page.goto("/build/processes");
  await expect(page.getByRole("heading", { name: "Processes" })).toBeVisible();

  const stamp = Date.now();
  const key = `pw-proc-${stamp}`;
  const title = `Playwright process ${stamp}`;

  // Every step is judged against a published SOP; the publish rule refuses a step without one.
  // The SOP is its own versioned document and is authored through its own real actions.
  const sop = await invoke<{ id: string }>(
    request,
    "procedures.create",
    {
      key: `pw-sop-${stamp}`,
      title: `Playwright SOP ${stamp}`,
      acceptance_criteria: ["the body is posted"],
      inspection: { kind: "checker", rule: "always" },
    },
    undefined,
    "bob",
  );
  await invoke(request, "procedures.publish", { procedure_id: sop.id }, undefined, "bob");

  await page.getByLabel("Key").fill(key);
  await page.getByLabel("Title").fill(title);
  await page.getByLabel("Card type").fill("bug");
  await page.getByRole("button", { name: "Create draft" }).click();
  await expect(page.getByText(`Created draft ${key}/v1.`)).toBeVisible();

  // The register is a page of 100, and the seeded catalogue fills it; the draft filter is the
  // user's own way to reach a row that was just created (the catalogue seeds only published ones).
  await page.getByLabel("Filter by status").selectOption("draft");
  await page.getByRole("link", { name: title }).click();
  await expect(page.getByRole("heading", { name: title })).toBeVisible();
  await expect(page.locator(".pill", { hasText: "draft" }).first()).toBeVisible();
  const processId = new URL(page.url()).pathname.split("/").filter(Boolean).pop()!;

  // Every step is judged against a published SOP; the fixture above is that SOP.
  const sopId = sop.id;

  // Two steps through the real processes.update: whole-list replace under If-Match.
  await page.getByRole("button", { name: "Add step" }).click();
  await page.getByLabel("Step 1 kind").fill("computed");
  await page.getByLabel("Step 1 actor").fill("tool_agent");
  await page.getByLabel("Step 1 action").fill("cards.comment");
  await page.getByLabel("Step 1 SOP").fill(sopId);
  await page
    .getByLabel("Step 1 input template")
    .fill('{"id":"{{card.id}}","body":"hello from the Playwright gate"}');
  await page.getByRole("button", { name: "Add step" }).click();
  await page.getByLabel("Step 2 kind").fill("computed");
  await page.getByLabel("Step 2 action").fill("cards.nope");
  await page.getByLabel("Step 2 SOP").fill(sopId);
  await page.getByLabel("Step 2 input template").fill("{}");
  await page.getByRole("button", { name: "Save steps" }).click();
  await expect(page.getByText(/Saved 2 steps at lock v/)).toBeVisible();
  await expect(page.locator("table.steps tbody tr")).toHaveCount(2);

  // The action's own answer, not the screen's optimism: two stored steps with their kinds.
  const saved = await invoke<ProcessView>(request, "processes.get", { process_id: processId }, undefined, "bob");
  expect(saved.steps.map((step) => step.action)).toEqual(["cards.comment", "cards.nope"]);
  expect(saved.steps.map((step) => step.kind)).toEqual(["computed", "computed"]);
  // A blank actor is defaulted by the domain, not left empty: a computed step is a tool agent's.
  expect(saved.steps.map((step) => step.actor)).toEqual(["tool_agent", "tool_agent"]);

  // Publish is refused by the API with a 422; the warning panel repeats it verbatim.
  await page.getByRole("button", { name: "Publish" }).click();
  const refusal = page.locator('[role="alert"]');
  await expect(refusal).toContainText("422");
  await expect(refusal).toContainText('"cards.nope" is not a registered action');
  await expect(
    invoke(request, "processes.publish", { process_id: processId }, undefined, "bob"),
  ).rejects.toThrow(/process\.action_unknown/);

  // Fix the step. The draft is still editable and the second save moves the lock version.
  await page.getByLabel("Step 2 action").fill("cards.comment");
  await page.getByRole("button", { name: "Save steps" }).click();
  await expect
    .poll(async () => {
      const current = await invoke<ProcessView>(request, "processes.get", {
        process_id: processId,
      }, undefined, "bob");
      return current.steps[1]?.action;
    })
    .toBe("cards.comment");
  const fixed = await invoke<ProcessView>(request, "processes.get", {
    process_id: processId,
  }, undefined, "bob");
  expect(fixed.lock_version).toBeGreaterThan(saved.lock_version);

  await page.getByRole("button", { name: "Publish" }).click();
  await expect(page.getByText(/Published version 1/)).toBeVisible();
  await expect(page.locator(".pill", { hasText: "published" }).first()).toBeVisible();

  // A run names a card and a published version. The card is created through the real board
  // actions, and the screen's answer is the run's own link.
  const fixture = await createBoardCard(request, `process card ${stamp}`, "bob");
  await page.getByLabel("Card id").fill(fixture.card.id);
  await page.getByRole("button", { name: "Start run" }).click();
  const runLink = page.getByRole("link", { name: /Open run/ });
  await expect(runLink).toBeVisible();
  await expect(runLink).toHaveAttribute("href", /^\/runs\/[0-9a-f-]{36}$/);

  // Retire is the third declared transition; a live run is unaffected by it.
  await page.getByRole("button", { name: "Retire" }).click();
  await expect(page.getByText(/Retired version 1/)).toBeVisible();
  await expect(page.locator(".pill", { hasText: "retired" }).first()).toBeVisible();

  await expect(page.locator(".problem")).toHaveCount(0);
});

test("a seeded process shows its steps' kinds and the SOP each is judged against", async ({
  page,
  request,
}) => {
  // The shipped catalogue (migrate --process-catalogue) is the fixture: one of its processes
  // carries steps with a SOP, so the screen's new panel has real data without a hand-made row.
  const pageOut = await invoke<{ items: ProcessView[] }>(request, "processes.list", {
    limit: 200,
  });
  const seeded = pageOut.items.find((process) => process.steps.some((step) => step.procedure_id));
  expect(seeded, "the catalogue seeds a process with a SOP").toBeTruthy();

  await page.goto(`/build/processes/${seeded!.id}`);
  await expect(page.getByRole("heading", { name: seeded!.title })).toBeVisible();
  await expect(page.getByRole("columnheader", { name: "Kind", exact: true })).toBeVisible();
  await expect(page.getByRole("columnheader", { name: "SOP", exact: true })).toBeVisible();

  // The SOP panel reads procedures.get for the step's SOP and shows the judged fields.
  await expect(page.getByText(/SOP · v\d+ ·/).first()).toBeVisible();
  await expect(page.getByText("Ready", { exact: true }).first()).toBeVisible();
  await expect(page.getByText("Acceptance", { exact: true }).first()).toBeVisible();
  await expect(page.getByText("Inspection", { exact: true }).first()).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("an empty tenant says so and offers the create form", async ({ page }) => {
  // fr-test is a member of `execution` and `kata`: real tenants with the catalogue but no
  // process catalogue. The shell defaults to the first membership, so the register is empty.
  await as(page, "fr-test");
  await page.goto("/build/processes");
  await expect(page.getByRole("heading", { name: "Processes" })).toBeVisible();
  await expect(page.getByText("No process matches this filter. Create one below.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Create draft" })).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("a process id from nowhere is a 404 panel, not a blank page", async ({ page }) => {
  await page.goto("/build/processes/00000000-0000-0000-0000-000000000000");
  await expect(page.getByRole("heading", { name: "That did not load" })).toBeVisible();
  await expect(page.locator(".problem")).toContainText("not found");
});

test("the process screens have no horizontal page scroll at 1280 and 390", async ({ page }) => {
  await page.goto("/build/processes");
  await expect(page.getByRole("heading", { name: "Processes" })).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `width ${width}`).toBeLessThanOrEqual(0);
  }
});
