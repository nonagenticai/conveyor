import { expect, test, type APIRequestContext, type Page } from "@playwright/test";

import { createBoardCard, invoke } from "./helpers";

const baseURL = process.env.BASE_URL ?? "http://127.0.0.1:8931";

/** Swap the dev cookie. The file's storageState is alice; bob is the empty tenant. */
async function as(page: Page, login: string) {
  const host = new URL(baseURL).hostname;
  await page.context().addCookies([
    { name: "conveyor_dev", value: login, domain: host, path: "/" },
  ]);
}

type ArchitectureFixture = {
  modelId: string;
  modelTitle: string;
  requirementId: string;
  requirementTitle: string;
  cardId: string;
};

/**
 * A model with three elements, one relationship and one generated view — all through the real
 * actions. Two elements name governed assets: one is a requirement that exists (and whose
 * title the element repeats), the other names an asset that does not, which is the validator's
 * deterministic `governed_asset_missing` finding. The trace link is the asset/card edge.
 */
async function seedArchitecture(request: APIRequestContext): Promise<ArchitectureFixture> {
  const stamp = Date.now();
  const modelTitle = `Playwright architecture ${stamp}`;
  const model = await invoke<{ id: string }>(request, "models.create", {
    title: modelTitle,
    language: "archimate",
    language_version: "3.2",
  });
  const requirement = await invoke<{ id: string; title: string }>(request, "requirements.create", {
    title: `Playwright architecture requirement ${stamp}`,
  });

  const service = await invoke<{ id: string }>(request, "model_elements.create", {
    model_id: model.id,
    kind: "application_component",
    name: `Payments service ${stamp}`,
  });
  const actor = await invoke<{ id: string }>(request, "model_elements.create", {
    model_id: model.id,
    kind: "business_actor",
    name: requirement.title,
    meta: { governed_asset_id: requirement.id },
  });
  await invoke(request, "model_elements.create", {
    model_id: model.id,
    kind: "application_component",
    name: `Unlinked service ${stamp}`,
    // A well-formed id that resolves to no governed asset in this tenant: the validator's
    // `governed_asset_missing` finding, produced by the worker, not by the screen.
    meta: { governed_asset_id: "00000000-0000-0000-0000-0000000000aa" },
  });
  await invoke(request, "model_relationships.create", {
    model_id: model.id,
    kind: "serving",
    source_id: service.id,
    target_id: actor.id,
  });

  const view = await invoke<{ id: string; version: number }>(request, "model_views.create", {
    model_id: model.id,
    name: "Main diagram",
  });
  await invoke(request, "model_views.generate", { view_id: view.id }, view.version);
  await invoke(request, "models.validate", { id: model.id });

  const board = await createBoardCard(request, `Playwright architecture card ${stamp}`);
  await invoke(request, "trace_links.create", {
    from_type: "governed_asset",
    from_id: requirement.id,
    to_type: "card",
    to_id: board.card.id,
    kind: "traces",
  });

  return {
    modelId: model.id,
    modelTitle,
    requirementId: requirement.id,
    requirementTitle: requirement.title,
    cardId: board.card.id,
  };
}

test("a model is listed, drawn, validated and traced", async ({ page, request }) => {
  const fixture = await seedArchitecture(request);

  // ── the list: the model the action wrote is a row, and its title is the link ──────────
  await page.goto("/design/models");
  await expect(page.getByRole("heading", { name: "Architecture" })).toBeVisible();
  const row = page.getByRole("link", { name: fixture.modelTitle });
  await expect(row).toBeVisible();
  await row.click();

  // ── the detail: header, the view selector and the rendered diagram ───────────────────
  await expect(page.getByRole("heading", { name: fixture.modelTitle })).toBeVisible();
  await expect(page.getByRole("combobox", { name: /View/ })).toContainText("Main diagram");

  // The SVG arrives as a blob URL streamed with the caller's credential: an <img src>
  // cannot carry the Authorization header, which is why the screen fetches it itself.
  const diagram = page.locator('img[alt="Rendered model"]');
  await expect(diagram).toBeVisible({ timeout: 20_000 });
  await expect(diagram).toHaveAttribute("src", /^blob:/, { timeout: 20_000 });
  // The scheme alone proves nothing: a zero-byte blob still has a blob src. The decoded
  // width is the evidence that the renderer produced a diagram the browser can draw.
  await expect
    .poll(
      async () =>
        diagram.evaluate((element) => {
          const image = element as HTMLImageElement;
          return image.complete ? image.naturalWidth : 0;
        }),
      { timeout: 20_000, intervals: [250, 500, 1000] },
    )
    .toBeGreaterThan(0);

  // ── validation: the button queues a run; the worker records the findings ─────────────
  await page.getByRole("button", { name: "Validate" }).click();
  await expect(page.getByRole("status")).toContainText(/validation/i);

  const validation = page.getByRole("region", { name: "Validation" });
  await expect
    .poll(
      async () => {
        await page.getByRole("button", { name: "Refresh" }).click();
        return validation.getByText("governed_asset_missing").count();
      },
      { timeout: 60_000, intervals: [1_000, 2_000] },
    )
    .toBeGreaterThan(0);

  // ── the traceability matrix: every element, with its edge counts ─────────────────────
  const matrix = page.getByRole("region", { name: "Traceability matrix" });
  await expect(matrix.locator("tbody tr")).toHaveCount(3);
  await expect(matrix).toContainText(fixture.requirementTitle);
  await expect(matrix).toContainText("untraced");

  // ── one trace link: the asset/card edge the spec wrote ───────────────────────────────
  const links = page.getByRole("region", { name: "Trace links" });
  await expect(links.locator("tbody tr")).toHaveCount(1);
  await expect(links).toContainText("traces");
  await expect(links).toContainText(fixture.cardId.slice(0, 8));

  // ── no horizontal page scroll on the wide screen, at both measured widths ────────────
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `detail width ${width}`).toBeLessThanOrEqual(0);
  }
});

test("an empty tenant says so and offers the create form", async ({ page }) => {
  await as(page, "bob"); // dev-b: a real tenant with no models
  await page.goto("/design/models");
  await expect(page.getByRole("heading", { name: "Architecture" })).toBeVisible();
  await expect(page.getByText("No model in this tenant yet.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Create model" })).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("a model id from nowhere is a 404 panel, not a blank page", async ({ page }) => {
  await page.goto("/design/models/00000000-0000-0000-0000-000000000000");
  await expect(page.getByRole("heading", { name: "That did not load" })).toBeVisible();
  await expect(page.locator(".problem")).toContainText("not found");
});

test("the architecture list has no horizontal page scroll at 1280 and 390", async ({ page }) => {
  await page.goto("/design/models");
  await expect(page.getByRole("heading", { name: "Architecture" })).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `width ${width}`).toBeLessThanOrEqual(0);
  }
});
