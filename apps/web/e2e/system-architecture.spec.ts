import { expect, test } from "./fixture";

import { isDevAuth, invoke } from "./helpers";

// #566 T18-5 (W5) — System architecture at `/architecture`: the tenant's ArchiMate models, their
// generated views and rendered diagrams, the element-level traceability matrix, and the build's
// UI map. The fixture is created through the real actions; the screen renders no fixture row.

let sequence = 0;
function unique(prefix: string): string {
  sequence += 1;
  return `${prefix} ${Date.now()}-${sequence}`;
}

test("the map renders the model, its generated diagram and the traceability matrix", async ({
  page,
  request,
}) => {
  const title = unique("e2e system map");
  const model = await invoke<{ id: string }>(request, "models.create", {
    title,
    language: "archimate",
    language_version: "3.2",
  });
  await invoke(request, "model_elements.create", {
    model_id: model.id,
    kind: "application_component",
    name: unique("e2e payments service"),
  });
  const view = await invoke<{ id: string; version: number }>(request, "model_views.create", {
    model_id: model.id,
    kind: "archimate_view",
    name: `${title} · view`,
  });
  await invoke(request, "model_views.generate", { view_id: view.id }, view.version);

  await page.goto("/architecture");

  await expect(page.locator('[data-testid="system-architecture"]')).toBeVisible();
  // Other specs create models in the same database; select this test's row explicitly rather
  // than relying on the newest-first default.
  const row = page
    .locator('[data-testid="system-models"] tr')
    .filter({ hasText: title })
    .first();
  await expect(row).toBeVisible();
  await row.getByRole("button", { name: "Map" }).click();
  await expect(page.locator('[data-testid="system-views"]')).toContainText(`${title} · view`);
  // The generated view is drawn through GET /api/models/{id}/render as an object URL.
  await expect(page.locator('[data-testid="system-views"] img')).toBeVisible();
  await expect(page.locator('[data-testid="system-traceability"]')).toContainText("1 element");
  await expect(page.locator('[data-testid="system-ui-map"]')).toContainText("/design/adm");
});

test("a tenant without the read gets the honest gate, not a fixture", async ({
  page,
  context,
  baseURL,
}) => {
  test.skip(!isDevAuth(), "the deployed suite has one principal; the empty tenant is a dev seed");
  const host = new URL(baseURL!).hostname;
  await context.addCookies([{ name: "conveyor_dev", value: "bob", domain: host, path: "/" }]);

  await page.goto("/architecture");
  await expect(page.locator('[data-testid="system-architecture"]')).toBeVisible();
  // Either the role's manifest has no models.list (the Planned panel) or the tenant has no model
  // (the screen's own empty state). Both are honest; neither is a fixture row.
  await expect(
    page.locator(".planned-panel").or(page.locator('[data-testid="system-models"]')),
  ).toBeVisible();
});
