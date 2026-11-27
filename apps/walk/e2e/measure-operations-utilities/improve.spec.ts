import { expect, test } from "@playwright/test";

// Improve: the plan gate, the kata lookup, the acts as sheet opens, a read failure and the
// lesson write.

test.describe.configure({ mode: "serial" });

const KATA = { get: "the six gets", content: { target: "delivery", measurement: "cycle time" }, recorded_by: "alice", recorded_at: "2026-11-27T10:00:00Z" };

function json(route: any, body: unknown) {
  return route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ output: body }) });
}

test("without kata.open the screen is the plan gate naming it", async ({ page }) => {
  await page.route(/\/api\/actions$/, async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    body.actions = (body.actions ?? []).filter((entry: any) => entry.name !== "kata.open");
    await route.fulfill({ response, body: JSON.stringify(body) });
  });
  await page.goto("/improve");
  await expect(page.getByRole("heading", { name: "Improve is planned" })).toBeVisible();
  await expect(page.getByText(/kata\.open is not in this role's manifest/)).toBeVisible();
});

test("an improvement lookup renders the kata", async ({ page }) => {
  await page.route("**/api/actions/kata.get/invocations", (route: any) => json(route, KATA));
  await page.goto("/improve");
  await expect(page.getByRole("heading", { name: "Improve", level: 1 })).toBeVisible();
  await page.getByLabel("Improvement id").fill("00000000-0000-4000-8000-000000000f01");
  await page.getByRole("button", { name: "Look up" }).click();
  await expect(page.getByTestId("improve-kata")).toContainText("the six gets");
});

test("the kata acts open their sheets", async ({ page }) => {
  await page.route("**/api/actions/kata.get/invocations", (route: any) => json(route, KATA));
  await page.goto("/improve");
  await page.getByLabel("Improvement id").fill("00000000-0000-4000-8000-000000000f01");
  await page.getByRole("button", { name: "Look up" }).click();
  await page.getByRole("button", { name: "Measure" }).click();
  const dialog = page.getByRole("dialog", { name: "Measure" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Submit" })).toBeVisible();
});

test("the open write opens its sheet", async ({ page }) => {
  await page.goto("/improve");
  await page.getByRole("button", { name: "Open an improvement" }).click();
  const dialog = page.getByRole("dialog", { name: "Open an improvement" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Submit" })).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route("**/api/actions/kata.get/invocations", (route: any) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({ title: "Internal Server Error", status: 500, detail: "the kata read failed" }),
    }),
  );
  await page.goto("/improve");
  await page.getByLabel("Improvement id").fill("00000000-0000-4000-8000-000000000f01");
  await page.getByRole("button", { name: "Look up" }).click();
  await expect(page.locator(".problem")).toContainText("the kata read failed");
});
