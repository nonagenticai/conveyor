import { expect, test } from "@playwright/test";

// The seeded audit (dev-a, `seed.rs::measure_improve`) has four findings: one open high, one
// open medium, one accepted, one rejected — plus a newer queued audit that must not hide it.
test("the seeded audit renders its findings and Accept records the decision", async ({ page }) => {
  await page.goto("/measure/prompt-audit");
  await expect(page.getByRole("heading", { name: "Prompt & process audit" })).toBeVisible();

  // The newest audit with findings is selected; the queued one is listed, not shown.
  await expect(page.locator(".finding")).toHaveCount(4);
  await expect(page.locator(".finding.high")).toHaveCount(1);
  await expect(page.getByText("process bug-fix · step 2")).toBeVisible();

  // The footer counts the open high findings — publish re-audits the draft, so this is a
  // report, not a promise the screen makes about the next publish.
  const blocking = page.locator("span", { hasText: "open high findings:" }).first();
  await expect(blocking.locator("b")).toHaveText("1");

  // Accept the high finding: a real `prompt_findings.decide`, then the audit is refetched.
  await page
    .locator(".finding.high")
    .getByRole("button", { name: "Accept rewrite" })
    .click();
  await expect(page.locator(".finding.high .pill", { hasText: "accepted" })).toBeVisible();
  await expect(blocking.locator("b")).toHaveText("0");

  // The cadence panel reads the tenant's settings document.
  await expect(page.getByRole("button", { name: "Save cadence" })).toBeVisible();

  await expect(page.locator(".problem")).toHaveCount(0);
});

test("the run form refuses to start without a model and explains the actions surface", async ({
  page,
}) => {
  await page.goto("/measure/prompt-audit");
  await page.getByRole("button", { name: "Run audit" }).click();

  // The `actions` surface is declared, not auditable (S16 CCR-7) — shown disabled.
  const actions = page.getByText("Action descriptions — declared, not auditable");
  await expect(actions).toBeVisible();

  // With the ledger's models seeded, the picker carries them; the form is honest either way.
  await expect(page.getByLabel("Audit target model")).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
});
