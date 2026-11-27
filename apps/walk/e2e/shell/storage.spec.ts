import { expect, test } from "@playwright/test";

// The walk has no programme store of its own, and it never reads the published snapshot: the
// read models come from the API (SPEC §14).

test("no conveyor.gemba storage and no snapshot fetch", async ({ page }) => {
  const snapshots: string[] = [];
  page.on("request", (request) => {
    if (request.url().includes("snapshot")) snapshots.push(request.url());
  });
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Today" })).toBeVisible();
  const local = await page.evaluate(() =>
    Object.keys(window.localStorage).filter((key) => key.startsWith("conveyor.gemba")),
  );
  const session = await page.evaluate(() =>
    Object.keys(window.sessionStorage).filter((key) => key.startsWith("conveyor.gemba")),
  );
  expect(local).toEqual([]);
  expect(session).toEqual([]);
  expect(snapshots).toEqual([]);
});
