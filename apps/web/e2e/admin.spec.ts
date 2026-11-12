import { expect, test, type Page } from "@playwright/test";

import { invoke } from "./helpers";

const baseURL = process.env.BASE_URL ?? "http://127.0.0.1:8931";

/** Swap the dev cookie. The file's storageState is alice; bob is the empty tenant. */
async function as(page: Page, login: string) {
  const host = new URL(baseURL).hostname;
  await page.context().addCookies([
    { name: "conveyor_dev", value: login, domain: host, path: "/" },
  ]);
}

test("the registry, its schema, the audit trail, the licence and the honest externals", async ({
  page,
  request,
}) => {
  // A real write first, so the audit tab has something this test owns.
  const title = `Admin audit probe ${Date.now()}`;
  await invoke(request, "requirements.create", { title });

  await page.goto("/settings/admin");
  await expect(page.getByRole("heading", { name: "Admin" })).toBeVisible();

  // The registry: actions.list rows, and actions.describe's own schema on click.
  await expect(page.locator("table.table tbody tr").first()).toBeVisible();
  await page.getByRole("button", { name: "requirements.list" }).click();
  await expect(page.getByRole("heading", { name: "Input schema" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Output schema" })).toBeVisible();

  // The audit log is append-only and tenant-scoped: the create above is in it.
  await page.getByRole("button", { name: "Audit", exact: true }).click();
  await expect(page.getByText("requirements.create").first()).toBeVisible();

  // The licence is the synced entitlement, read and never written here.
  await page.getByRole("button", { name: "Licence", exact: true }).click();
  await expect(page.locator("table", { hasText: "App" })).toContainText("conveyor");
  await expect(page.getByText("checked")).toBeVisible();

  // Workers and billing are external services that dev does not configure. The tab must
  // name the reason, never draw zeros that look like a healthy empty result.
  await page.getByRole("button", { name: "Workers", exact: true }).click();
  await expect(page.locator(".problem")).toBeVisible();

  await expect(page.locator(".problem")).toContainText("no base URL is configured");
});

test("a second tenant reads its own manifest and its own audit trail", async ({ page }) => {
  // dev-b is a real tenant: the earlier specs wrote audit rows as bob, so the tab is a
  // tenant-scoped read rather than an empty page.
  await as(page, "bob");
  await page.goto("/settings/admin");
  await expect(page.getByRole("heading", { name: "Admin" })).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);

  await page.getByRole("button", { name: "Audit", exact: true }).click();
  await expect(page.locator("table.table").first()).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("the admin screen has no horizontal page scroll at 1280 and 390", async ({ page }) => {
  await page.goto("/settings/admin");
  await expect(page.getByRole("heading", { name: "Admin" })).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `width ${width}`).toBeLessThanOrEqual(0);
  }
});
