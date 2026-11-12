import { expect, test } from "@playwright/test";

import { invoke } from "./helpers";

// Every fixture is created through the real actions and found again through `search.query`;
// nothing is route-mocked, so a hit here means the ranked statement saw the row.

test("a requirement created through the action is found under governed assets and opens", async ({
  page,
  request,
}) => {
  // A single alphanumeric token is both an FTS term and a trigram-safe word, so the row is
  // reachable whichever arm's predicate answers — and it cannot collide with the seed.
  const token = `zzsearch${Date.now()}`;
  const title = `Search gate ${token}`;
  const asset = await invoke<{ id: string; title: string }>(request, "requirements.create", {
    title,
  });

  await page.goto("/search");
  await expect(page.getByRole("heading", { name: "Search", exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Type to search" })).toBeVisible();

  await page.getByLabel("Search query").fill(token);
  await page.getByRole("button", { name: "Search", exact: true }).click();

  // The hit is grouped under the governed-asset arm, not listed flat and not under Cards.
  const governed = page.locator("section.panel", { hasText: "Governed assets" });
  await expect(governed.getByRole("link", { name: title })).toBeVisible();
  await expect(page.getByText("comparable only within this response")).toBeVisible();

  // Only a requirement has a page: the link lands on its detail route.
  await governed.getByRole("link", { name: title }).click();
  await expect(page).toHaveURL(new RegExp(`/design/requirements/${asset.id}$`));
  await expect(page.getByRole("heading", { name: title })).toBeVisible();

  await expect(page.locator(".problem")).toHaveCount(0);
});

test("an empty query says type to search instead of sending the action", async ({ page }) => {
  await page.goto("/search");
  await expect(page.getByRole("heading", { name: "Type to search" })).toBeVisible();

  // Search with nothing typed explains itself rather than sending an empty `q` (a 422).
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.getByText("Type at least one character to search.")).toBeVisible();
  await expect(page.getByRole("heading", { name: "Type to search" })).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("a query with no hit says so instead of showing a stale list", async ({ page }) => {
  await page.goto("/search");
  await page.getByLabel("Search query").fill(`qzxv${Date.now()}jkw`);
  await page.getByRole("button", { name: "Search", exact: true }).click();

  await expect(page.getByRole("heading", { name: "No results" })).toBeVisible();
  await expect(page.locator("section.panel", { hasText: "Governed assets" })).toHaveCount(0);
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("the search screen has no horizontal page scroll at 1280 and 390", async ({ page }) => {
  await page.goto("/search");
  await expect(page.getByRole("heading", { name: "Search", exact: true })).toBeVisible();

  // Populate the result table first: it is the widest element the screen ever draws.
  await page.getByLabel("Search query").fill("requirement");
  await page.getByRole("button", { name: "Search", exact: true }).click();
  await expect(page.locator("section.panel").first()).toBeVisible();

  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `width ${width}`).toBeLessThanOrEqual(0);
  }
});
