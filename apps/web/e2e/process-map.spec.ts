import { expect, test } from "@playwright/test";

// Screen 22 — the process map. Two reads only (`process_maps.list` / `process_maps.get`),
// against the shipped catalogue: `migrate --process-catalogue` installs one versioned map
// per project type, so the list has a real row without a hand-made fixture.
//
// What this spec deliberately does NOT assert: a live position on a node. No action joins a
// project's runs to the map's nodes yet, and the screen says so; inventing a badge here would
// be the one thing the map must never do.

test("the seeded map draws its nodes and edges, and names the missing live position", async ({
  page,
}) => {
  await page.goto("/build/process-map");
  await expect(page.getByRole("heading", { name: "Process map", exact: true })).toBeVisible();

  // The catalogue's Py2Rust map: a five-phase document.
  const row = page.locator("tbody tr", { hasText: "py2rust" });
  await expect(row).toBeVisible();
  await row.getByRole("link", { name: "Py2Rust" }).click();

  await expect(page).toHaveURL(/\/build\/process-map\/[0-9a-f-]+$/);
  await expect(page.getByRole("heading", { name: "Py2Rust" })).toBeVisible();

  // The DAG is drawn: five nodes, four wires, and the labels are the document's own.
  const svg = page.locator("svg.process-map");
  await expect(svg).toBeVisible();
  await expect(svg.locator("rect.pm-node")).toHaveCount(5);
  await expect(svg).toContainText("Setup");
  await expect(svg).toContainText("Plan");
  await expect(svg).toContainText("Port");
  await expect(svg).toContainText("Parity");
  await expect(svg).toContainText("Cutover");
  await expect(svg.locator("line.pm-wire")).toHaveCount(4);

  // The declarative table carries the same four edges.
  await expect(page.locator("table").last().locator("tbody tr")).toHaveCount(4);

  // The gap is stated with its reason, never papered over with a guessed state.
  await expect(page.getByRole("heading", { name: "Live position" })).toBeVisible();
  await expect(
    page.getByText("no action joins a project's runs to this map's nodes yet"),
  ).toBeVisible();

  // And every number on the screen came from the action, not from a mock.
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("an unknown map is a problem panel, not a blank canvas", async ({ page }) => {
  await page.goto("/build/process-map/00000000-0000-0000-0000-000000000000");
  await expect(page.getByRole("heading", { name: "That did not load" })).toBeVisible();
  await expect(page.locator("svg.process-map")).toHaveCount(0);
});
