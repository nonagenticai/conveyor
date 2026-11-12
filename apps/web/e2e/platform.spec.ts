import { expect, test, type Page } from "@playwright/test";

// S12 wave 2, the Platform surface. Two real modes, no mocked API:
//   * a tenant admin is refused the surface;
//   * a super-admin (the seeded `fr-test`) sees the tenants, opens one, and reads the
//     honest unconfigured states — provisioning and billing are NOT configured in the dev
//     stack, so the screen must say so rather than draw a green panel with zeros.

const baseURL = process.env.BASE_URL ?? "http://127.0.0.1:8931";

/** Swap the dev cookie. The file's storageState is alice (admin of dev-a, NOT platform). */
async function as(page: Page, login: string) {
  const host = new URL(baseURL).hostname;
  await page.context().addCookies([
    { name: "conveyor_dev", value: login, domain: host, path: "/" },
  ]);
}

test("a tenant admin is refused and a super-admin sees the tenants", async ({ page }) => {
  await page.goto("/platform");
  await expect(page.getByText("The Platform surface is super-admin only.")).toBeVisible();

  await as(page, "fr-test");
  await page.reload();
  await expect(page.getByRole("heading", { name: "Platform", level: 1 })).toBeVisible();
  await expect(page.getByRole("button", { name: "New tenant" })).toBeVisible();

  // The dev seed gives fr-test the `kata` and `execution` tenants.
  await expect(page.locator("table.table tbody tr", { hasText: "kata" })).toHaveCount(1);
  await expect(page.locator("table.table tbody tr", { hasText: "execution" })).toHaveCount(1);
});

test("the wizard renders and the tenant detail shows admins and honest billing", async ({
  page,
}) => {
  await as(page, "fr-test");
  await page.goto("/platform");

  // The wizard form, not a fake button.
  await page.getByRole("button", { name: "New tenant" }).click();
  await expect(page.getByText("First admin email")).toBeVisible();
  await expect(page.getByRole("button", { name: "Create tenant" })).toBeVisible();

  // Open the seeded kata tenant: its admin is fr-test, and billing is unconfigured in the
  // dev stack — the panel must name the reason, never render zeros.
  await page.locator("table.table tbody tr", { hasText: "kata" }).click();
  await expect(page.getByRole("heading", { name: "Kata", level: 2 })).toBeVisible();
  await expect(page.getByText("fr-test@dev.invalid")).toBeVisible();
  await expect(page.getByText("Billing is unavailable", { exact: false })).toBeVisible();
  // One reason per source (billing, costs, actors) — all three name the same absent setting.
  await expect(page.getByText("BILLING_BASE_URL is not configured")).toHaveCount(3);

  // Advanced deepens the detail panel with the ids.
  await page.getByRole("button", { name: /Advanced/ }).click();
  await expect(page.getByText("Ids and versions")).toBeVisible();
});

test("the platform screen has no horizontal page scroll at 1280 and 390", async ({ page }) => {
  await as(page, "fr-test");
  await page.goto("/platform");
  await expect(page.getByRole("heading", { name: "Platform", level: 1 })).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth > document.documentElement.clientWidth,
    );
    expect(overflow, `horizontal scroll at ${width}px`).toBe(false);
  }
});
