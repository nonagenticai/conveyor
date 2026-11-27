import { expect, test } from "@playwright/test";

// Copilot: the route-transported surface. The action list renders when the route answers, an
// empty route says so, and a refusal is the problem panel plus the hint naming S10 — never a
// fabricated conversation.

test.describe.configure({ mode: "serial" });

const ACTION = { name: "propose_change", summary: "Propose a change against a card" };

test("the copilot action surface renders from its route", async ({ page }) => {
  await page.route("**/api/copilot/actions", (route: any) =>
    route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ actions: [ACTION] }) }),
  );
  await page.goto("/copilot");
  await expect(page.getByRole("heading", { name: "Copilot", level: 1 })).toBeVisible();
  await expect(page.getByTestId("copilot-actions")).toContainText("propose_change");
  await expect(page.getByTestId("copilot-actions")).toContainText("Propose a change");
});

test("an empty route is the app's own message", async ({ page }) => {
  await page.route("**/api/copilot/actions", (route: any) =>
    route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ actions: [] }) }),
  );
  await page.goto("/copilot");
  await expect(page.getByText(/no action yet; this is the empty state/)).toBeVisible();
});

test("a refusal is the problem panel and the slice hint", async ({ page }) => {
  await page.route("**/api/copilot/actions", (route: any) =>
    route.fulfill({
      status: 403,
      contentType: "application/problem+json",
      body: JSON.stringify({ title: "Forbidden", status: 403, detail: "the copilot surface is not enabled for this role" }),
    }),
  );
  await page.goto("/copilot");
  await expect(page.locator(".problem")).toContainText("the copilot surface is not enabled for this role");
  await expect(page.getByText(/the copilot surface is served by the S10 slice/)).toBeVisible();
});
