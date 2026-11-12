import { expect, test, type Page } from "@playwright/test";

const baseURL = process.env.BASE_URL ?? "http://127.0.0.1:8931";

/** Swap the dev cookie. The file's storageState is alice; bob is the empty tenant. */
async function as(page: Page, login: string) {
  const host = new URL(baseURL).hostname;
  await page.context().addCookies([
    { name: "conveyor_dev", value: login, domain: host, path: "/" },
  ]);
}

test("a requirement is created, revised, moved and compared", async ({ page }) => {
  await page.goto("/design/requirements");
  await expect(page.getByRole("heading", { name: "Requirements" })).toBeVisible();

  const title = `Playwright requirement ${Date.now()}`;
  await page.getByLabel("Title").fill(title);
  await page.getByRole("button", { name: "Create draft" }).click();

  // The register lists the row the action wrote; clicking it opens the detail.
  await expect(page.getByRole("link", { name: title })).toBeVisible();
  await page.getByRole("link", { name: title }).click();
  await expect(page.getByRole("heading", { name: title })).toBeVisible();

  // An edit is a revision: save a body, and the history grows to two rows.
  const revisions = page.locator("section.panel", { hasText: "Revisions" });
  await expect(revisions.locator("tbody tr")).toHaveCount(1);
  await page.getByLabel("Body").fill("The statement, written by the Playwright gate.");
  await page.getByRole("button", { name: "Save revision" }).click();
  await expect(page.getByText("Saved as a new revision.")).toBeVisible();
  await expect(revisions.locator("tbody tr")).toHaveCount(2);

  // A transition is status-guarded; the permitted move is drawn from the current status.
  await page.getByRole("button", { name: "Submit for review" }).click();
  await expect(page.locator(".pill", { hasText: "in review" }).first()).toBeVisible();
  await expect(page.getByRole("button", { name: "Approve" })).toBeVisible();

  // Compare the oldest and newest revisions through requirements.compare.
  await page.getByRole("button", { name: "Compare" }).click();
  const diff = page.locator("section.panel", { hasText: "Compare revisions" });
  await expect(diff.locator("tbody tr").first()).toBeVisible();

  await expect(page.locator(".problem")).toHaveCount(0);
});

test("an empty tenant says so and offers the create form", async ({ page }) => {
  await as(page, "bob"); // dev-b: a real tenant with no requirements
  await page.goto("/design/requirements");
  await expect(page.getByText("No requirement matches this filter.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Create draft" })).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("a requirement id from nowhere is a 404 panel, not a blank page", async ({ page }) => {
  await page.goto("/design/requirements/00000000-0000-0000-0000-000000000000");
  await expect(page.getByRole("heading", { name: "That did not load" })).toBeVisible();
  await expect(page.locator(".problem")).toContainText("not found");
});

test("the requirements screens have no horizontal page scroll at 1280 and 390", async ({ page }) => {
  await page.goto("/design/requirements");
  await expect(page.getByRole("heading", { name: "Requirements" })).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `width ${width}`).toBeLessThanOrEqual(0);
  }
});
