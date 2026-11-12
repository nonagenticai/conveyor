import { expect, test, type Page } from "@playwright/test";

const baseURL = process.env.BASE_URL ?? "http://127.0.0.1:8931";

/** Swap the dev cookie. The file's storageState is alice (admin of dev-a). */
async function as(page: Page, login: string) {
  const host = new URL(baseURL).hostname;
  await page.context().addCookies([
    { name: "conveyor_dev", value: login, domain: host, path: "/" },
  ]);
}

test("members lists the seeded admin and invites an address", async ({ page }) => {
  await page.goto("/settings/members");
  await expect(page.getByRole("heading", { name: "Members", level: 1 })).toBeVisible();

  // dev-a's seeded member is Alice, the tenant admin.
  const aliceRow = page.locator("table.table tbody tr", { hasText: "Alice" });
  await expect(aliceRow).toHaveCount(1);
  await expect(aliceRow.locator(".pill", { hasText: "Tenant admin" })).toHaveCount(1);

  // A single member IS the tenant admin; the screen says so rather than offering a
  // control the last-admin guard would refuse.
  await expect(page.getByText("One member: this person is the tenant admin by default")).toBeVisible();
  await expect(page.locator("table.table select")).toHaveCount(0);

  // The invite is recorded and listed; nothing is emailed. A per-run address keeps the dev
  // database's leftover pending invites from making the count assertion non-deterministic.
  const invited = `carol+${Date.now()}@example.com`;
  await page.getByLabel("Email").fill(invited);
  await page.getByRole("button", { name: "Create invitation" }).click();
  await expect(page.getByText("Invitation recorded.")).toBeVisible();
  await expect(page.locator("table.table tbody tr", { hasText: invited })).toHaveCount(1);

  // Revoking is a real write: the pending row disappears. Scope the button to THIS row —
  // the dev database accumulates pending invites from other runs.
  await page
    .locator("table.table tbody tr", { hasText: invited })
    .getByRole("button", { name: "Revoke" })
    .click();
  await expect(page.locator("table.table tbody tr", { hasText: invited })).toHaveCount(0);
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("the members screen has no horizontal page scroll at 1280 and 390", async ({ page }) => {
  await page.goto("/settings/members");
  await expect(page.getByRole("heading", { name: "Members", level: 1 })).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    await expect
      .poll(async () =>
        page.evaluate(
          () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
        ),
      )
      .toBe(true);
  }
});
