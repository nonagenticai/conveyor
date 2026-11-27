import { expect, test } from "@playwright/test";

// A deep link with more than one path segment must boot the app: the document's own URLs are
// root-absolute, so `/portfolio/<id>` loads `/style.css`, `/src/main.js` and `/data/world.json`
// rather than their `/portfolio/...` siblings (which 404 and leave a blank page).

test("a multi-segment deep link boots the app with its assets", async ({ page }) => {
  const failed: string[] = [];
  page.on("response", (response) => {
    if (response.status() >= 400) {
      failed.push(`${response.status()} ${new URL(response.url()).pathname}`);
    }
  });

  await page.goto("/portfolio/00000000-0000-4000-8000-000000000000");
  await expect(page.getByRole("banner", { name: "Session" })).toBeVisible();
  await expect(page.locator("#screen > *")).not.toHaveCount(0);

  const assetFailures = failed.filter((line) =>
    /(style\.css|src\/main\.js|data\/world\.json)$/.test(line),
  );
  expect(assetFailures).toEqual([]);
});
