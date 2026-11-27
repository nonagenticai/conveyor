import { expect, test } from "@playwright/test";

// The sheet's refusal is the server's own text, including each field error — the same shape
// apps/web renders (`detail · field: detail`). `approval_policies.set` on a non-destructive
// action is a deterministic field refusal from the dev seed.

test("a refused action shows the server's field detail", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Palette" }).click();
  const palette = page.getByRole("dialog", { name: "Palette" });
  await palette.getByRole("searchbox", { name: "Search" }).fill("approval_policies.set");
  await palette.getByRole("button", { name: /approval_policies\.set/ }).first().click();

  const dialog = page.getByRole("dialog", { name: "approval_policies.set" });
  await expect(dialog).toBeVisible();
  await dialog.getByLabel(/^Action/).fill("programmes.create");
  await dialog.getByLabel(/^Approval/).fill("required");
  await dialog.getByLabel(/^Self approval/).fill("forbidden");
  await dialog.getByRole("button", { name: "Submit" }).click();

  await expect(dialog.locator(".form-error")).toContainText("no approval to waive");
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Close" }).click();
});
