import { expect, test } from "./fixture";

import { decideAsReviewer, invoke, isDevAuth } from "./helpers";

// SPEC_V2 §10.5/§5.7 — the live Authoring destination. `authoring.intakes` is the queue read,
// `authoring.assets` the catalogue read; intake, draft, review and publish are governed actions
// the screen invokes. Every fixture here is created through the real actions, so no seeded row
// can satisfy a scenario.
//
// Two measured constraints the specs follow rather than assume: the queue reads the recorded
// intakes (`authoring.intakes`), with the draft count joined from the catalogue by `intake_id`,
// and the dependency order is enforced when the draft is attempted, not at publish.

let sequence = 0;
function unique(prefix: string): string {
  sequence += 1;
  return `${prefix}-${Date.now()}-${sequence}`;
}

test("the authoring destination shows the live catalogue", async ({ page, request }) => {
  const key = unique("live-dept");
  const intake = await invoke<{ id: string }>(request, "authoring.intake", {
    brief: "The live-catalogue spec's intake",
    sources: [],
  });
  const draft = await invoke<{ id: string; key: string; status: string }>(
    request,
    "authoring.draft",
    {
      intake_id: intake.id,
      kind: "department",
      key,
      payload: { title: "Live department" },
    },
  );
  expect(draft.status).toBe("draft");

  await page.goto("/authoring");
  await expect(page.getByRole("heading", { name: "Authoring", exact: true })).toBeVisible();
  await page.getByRole("tab", { name: "Draft catalogue" }).click();

  await expect(page.locator(".problem")).toHaveCount(0);
  await expect(page.getByLabel("Kind", { exact: true })).toBeVisible();
  await expect(page.getByLabel("Status", { exact: true })).toBeVisible();
  await expect(page.locator(".tile")).toHaveCount(4);
  await expect(page.locator("table.table")).toHaveCount(1);
  await expect(
    page.getByRole("heading", { name: "From intake to publication", exact: true }),
  ).toBeVisible();

  // The row this test created through the real action: live data, not a fixture.
  await expect(page.locator("table tbody tr", { hasText: key })).toHaveCount(1);
});

test("the intake queue reads the tenant's rows", async ({ page, request }) => {
  const stamp = Date.now();
  const brief = `E2E queue brief ${stamp}`;
  const bareBrief = `E2E recorded brief ${stamp}`;
  const intake = await invoke<{ id: string }>(request, "authoring.intake", {
    brief,
    sources: ["document:sop-1"],
  });
  // A second intake with NO draft: only `authoring.intakes` can show it, so its row is the
  // proof that the queue reads the recorded intakes rather than the catalogue's `intake_id`.
  const bare = await invoke<{ id: string }>(request, "authoring.intake", {
    brief: bareBrief,
    sources: [],
  });
  const key = unique("queue-dept");
  await invoke(request, "authoring.draft", {
    intake_id: intake.id,
    kind: "department",
    key,
    payload: { title: "Queue department" },
  });

  await page.goto("/authoring");
  await expect(page.getByRole("tab", { name: "Intake queue" })).toHaveAttribute(
    "aria-selected",
    "true",
  );

  const queue = page.getByTestId("authoring-intakes");
  const row = queue.locator(`tr[data-intake="${intake.id}"]`);
  await expect(row).toBeVisible();
  await expect(row).toContainText(brief);
  await expect(row).toContainText("1 source(s)");
  await expect(row).toContainText("1 draft(s)");
  await expect(row).toContainText("department");

  // The draft-less intake: its brief and its open status come from the action, not a join.
  const bareRow = queue.locator(`tr[data-intake="${bare.id}"]`);
  await expect(bareRow).toBeVisible();
  await expect(bareRow).toContainText(bareBrief);
  await expect(bareRow).toContainText("0 source(s)");
  await expect(bareRow).toContainText("yet to draft");
  await expect(bareRow).toContainText("open");

  await page.getByRole("tab", { name: "Draft catalogue" }).click();
  await expect(page.getByTestId("authoring-table").locator("tr", { hasText: key })).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("an unpublished dependency refuses the draft", async ({ page, request }) => {
  const stamp = Date.now();
  const intake = await invoke<{ id: string }>(request, "authoring.intake", {
    brief: `E2E dependency brief ${stamp}`,
    sources: [],
  });
  const dependency = await invoke<{ id: string; status: string }>(request, "authoring.draft", {
    intake_id: intake.id,
    kind: "industry",
    key: unique("dep-industry"),
    payload: { title: "Unpublished industry" },
  });
  expect(dependency.status).toBe("draft");
  const key = unique("dep-dept");

  await page.goto("/authoring");
  await page.getByRole("tab", { name: "Draft catalogue" }).click();
  await page.getByLabel("Intake", { exact: true }).selectOption(intake.id);
  await page.getByLabel("Asset kind", { exact: true }).selectOption("department");
  await page.getByLabel("Key", { exact: true }).fill(key);
  await page.getByLabel("Depends on").fill(dependency.id);
  await page.getByRole("button", { name: "Draft the asset" }).click();

  // The server's refusal names the unpublished asset; the screen repeats it and creates nothing.
  const refusal = page.locator("p.notice[role='alert']");
  await expect(refusal).toContainText("Draft refused");
  await expect(refusal).toContainText("asset_dependency_unpublished");
  await expect(refusal).toContainText(dependency.id);
  await expect(page.getByTestId("authoring-table").locator("tr", { hasText: key })).toHaveCount(0);
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("a reviewed draft publishes behind its gate", async ({ page, request }) => {
  test.skip(!isDevAuth(), "the deployed suite has one OIDC principal, so nobody else can decide");
  const stamp = Date.now();
  const intake = await invoke<{ id: string }>(request, "authoring.intake", {
    brief: `E2E review brief ${stamp}`,
    sources: [],
  });
  const key = unique("review-cap");
  const asset = await invoke<{ id: string }>(request, "authoring.draft", {
    intake_id: intake.id,
    kind: "capability",
    key,
    payload: { title: "Reviewed capability" },
  });

  await page.goto("/authoring");
  await page.getByRole("tab", { name: "Draft catalogue" }).click();
  const row = page.getByTestId("authoring-table").locator("tr", { hasText: key });
  await expect(row).toBeVisible();
  await row.getByRole("button", { name: "Select" }).click();

  // The requester opens the gate; the screen never decides it.
  await page.getByRole("tab", { name: "Review & publish" }).click();
  await expect(page.getByTestId("authoring-review")).toContainText(key);
  await page.getByRole("button", { name: "Request the review gate" }).click();
  await expect(page.locator("p.notice[role='alert']")).toContainText("Review gate requested");

  const gates = await invoke<{ items: { id: string; gate: string; target?: string }[] }>(
    request,
    "approvals.list",
    { status: "pending", limit: 200 },
  );
  const gate = gates.items.find(
    (item) => item.gate === "authoring.review" && item.target === asset.id,
  );
  expect(gate, "the gate the screen requested").toBeTruthy();
  await decideAsReviewer(request, "approvals.decide", gate!.id);

  // The decision happened outside the browser, so the screen refetches before it can record it.
  await page.getByRole("button", { name: "Refresh" }).click();
  await expect(page.getByRole("button", { name: "Record the review" })).toBeEnabled();
  await page.getByRole("button", { name: "Record the review" }).click();
  await expect(page.locator("p.notice[role='alert']")).toContainText("Review recorded");

  await expect(page.getByRole("button", { name: "Publish the asset" })).toBeEnabled();
  await page.getByRole("button", { name: "Publish the asset" }).click();
  await expect(page.locator("p.notice[role='alert']")).toContainText("Published");

  await page.getByRole("tab", { name: "Draft catalogue" }).click();
  const published = page.getByTestId("authoring-table").locator("tr", { hasText: key });
  await expect(published.getByText("published", { exact: true })).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("the authoring screen has no horizontal page scroll at 1280 and 390", async ({ page }) => {
  await page.goto("/authoring");
  await expect(page.getByRole("heading", { name: "Authoring", exact: true })).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `width ${width}`).toBeLessThanOrEqual(0);
  }
});
