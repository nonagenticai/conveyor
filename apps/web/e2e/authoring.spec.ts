import { expect, test } from "./fixture";

import { invoke } from "./helpers";

// SPEC_V2 §10.5/§5.7 — the live Authoring destination. `authoring.assets` is the catalogue
// read; intake, draft, review and publish are governed actions, never this screen's write. This
// spec creates its own draft through the real actions, so a fixture cannot satisfy the row.

let sequence = 0;
function uniqueKey(): string {
  sequence += 1;
  return `live-dept-${Date.now()}-${sequence}`;
}

test("the authoring destination shows the live catalogue", async ({ page, request }) => {
  const key = uniqueKey();
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

  await expect(page.locator(".problem")).toHaveCount(0);
  await expect(page.getByLabel("Kind")).toBeVisible();
  await expect(page.getByLabel("Status")).toBeVisible();
  await expect(page.locator(".tile")).toHaveCount(4);
  await expect(page.locator("table.table")).toHaveCount(1);
  await expect(
    page.getByRole("heading", { name: "From intake to publication", exact: true }),
  ).toBeVisible();

  // The row this test created through the real action: live data, not a fixture.
  await expect(page.locator("table tbody tr", { hasText: key })).toHaveCount(1);
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
