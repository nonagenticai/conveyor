import { expect, test } from "@playwright/test";

import { createBoardCard, invoke } from "./helpers";

test("the cockpit claims, starts a run and approves a gate", async ({ page, request }) => {
  const title = `e2e cockpit ${Date.now()}`;
  const fixture = await createBoardCard(request, title);
  await invoke(request, "approvals.request", {
    gate: "e2e gate",
    kind: "human_approval",
    card_id: fixture.card.id,
    description: "created by the cockpit spec",
    required_decisions: 1,
  });

  await page.goto(`/card/${fixture.card.id}`);
  await expect(page.getByRole("heading", { name: title })).toBeVisible();
  await expect(page.getByText("The card")).toBeVisible();
  await expect(page.getByText("e2e gate")).toBeVisible();

  // claim carries If-Match; the action bar flips to Release only after the write succeeded
  await page.getByRole("button", { name: "Claim" }).click();
  await expect(page.getByRole("button", { name: "Release" })).toBeVisible({ timeout: 10_000 });

  // decide the pending gate; the pill moves off pending
  await page.getByRole("button", { name: "Approve" }).click();
  await expect(page.locator(".pill", { hasText: /approved/i })).toBeVisible({ timeout: 10_000 });

  // The decision history names the principal, the actor kind and the channel (the S12 read).
  await expect(page.locator(".pill", { hasText: "Alice · human · web" })).toBeVisible();

  // Edit the card under If-Match: the heading is the refetched name, the body the new text.
  const edited = `${title} (edited)`;
  await page.getByRole("button", { name: "Edit" }).click();
  await page.getByLabel("Title").fill(edited);
  await page.getByLabel("Description").fill("edited by the cockpit spec");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByRole("heading", { name: edited })).toBeVisible({ timeout: 10_000 });
  await expect(page.getByText("edited by the cockpit spec")).toBeVisible();

  // create + start a run; the Runs panel gains a row
  await page.getByRole("button", { name: "New run" }).click();
  await expect(
    page.locator(".cockpit section.panel", { hasText: "Runs" }).locator(".row-card"),
  ).not.toHaveCount(0, { timeout: 10_000 });
  await expect(page.locator(".problem")).toHaveCount(0);
});
