import { expect, test } from "./fixture";

import { invoke } from "./helpers";

// T18-1 P2: the Programme destination reads the tenant's programmes, never a fixture.

test("a created programme renders from the live read", async ({ page, request }) => {
  const name = `E2E programme ${Date.now()}`;
  await invoke(request, "programmes.create", { name });

  await page.goto("/programme");
  await expect(page.getByRole("heading", { name: "Programme" })).toBeVisible();
  await expect(page.getByRole("cell", { name })).toBeVisible();
  await expect(page.getByText("No programmes in this tenant yet.")).toHaveCount(0);
});
