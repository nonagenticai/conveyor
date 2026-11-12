import { expect, test } from "@playwright/test";

test("the matrix shows both numbers and locks admin and the kernel", async ({ page }) => {
  await page.goto("/settings/roles");
  await expect(page.getByRole("heading", { name: "Roles & permissions" })).toBeVisible();

  // Four columns: the two editable roles, then the fixed ones.
  await expect(page.getByRole("columnheader", { name: "Member" })).toBeVisible();
  await expect(page.getByRole("columnheader", { name: "Viewer" })).toBeVisible();
  await expect(page.getByRole("columnheader", { name: "Tenant admin" })).toBeVisible();
  await expect(page.getByRole("columnheader", { name: "Kata executor (machine)" })).toBeVisible();

  // Only member and viewer carry a checkbox, and only on a non-kernel row: admin,
  // kata_executor and every kernel scope render as fixed marks.
  const rowCount = await page.locator("table.matrix tbody tr").count();
  expect(rowCount).toBeGreaterThan(20);
  const lockedRows = await page.locator("table.matrix .pill", { hasText: "locked" }).count();
  expect(lockedRows).toBeGreaterThan(0);
  await expect(page.locator("table.matrix input[type=checkbox]")).toHaveCount(
    (rowCount - lockedRows) * 2,
  );

  // The kernel cannot be revoked, and the screen says so before the server has to.
  await expect(page.locator("tr", { hasText: "members · write" }).locator(".pill", { hasText: "locked" })).toHaveCount(1);
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("an override round-trips and Reset clears it", async ({ page }) => {
  await page.goto("/settings/roles");
  await expect(page.getByRole("heading", { name: "Roles & permissions" })).toBeVisible();

  const row = page.locator("table.matrix tbody tr", { hasText: "runs · write" });
  await expect(row).toHaveCount(1);
  const memberCell = row.locator("input[type=checkbox]").first();
  // Member holds runs:write by default (projects:write is admin-only in the compiled table).
  await expect(memberCell).toBeChecked();

  await memberCell.uncheck();
  await expect(page.getByText("1 pending change(s)")).toBeVisible();
  await page.getByRole("button", { name: "Save changes" }).click();
  await expect(page.getByText("Policy saved.")).toBeVisible();

  // The server's answer is the rendered state: the role no longer holds the scope, and the
  // cell is marked as differing from its default.
  await expect(memberCell).not.toBeChecked();
  await expect(row.locator(".dot")).toHaveCount(1);

  // Reset deletes the override; the compiled default applies again.
  await page.getByRole("button", { name: "Reset to defaults" }).click();
  await expect(page.getByText("Every override removed")).toBeVisible();
  await expect(memberCell).toBeChecked();
  await expect(row.locator(".dot")).toHaveCount(0);
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("Advanced shows the raw scope spelling", async ({ page }) => {
  await page.goto("/settings/roles");
  await expect(page.getByText("runs · write")).toBeVisible();
  await page.getByRole("button", { name: /Advanced/ }).click();
  await expect(page.locator("table.matrix .mono", { hasText: "runs:write" })).toBeVisible();
});
