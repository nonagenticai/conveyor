import { expect, test, type APIRequestContext } from "@playwright/test";

import { invoke } from "./helpers";

async function firstEval(
  request: APIRequestContext,
): Promise<{ id: string; versionId: string }> {
  const page = await invoke<{
    items: { id: string; name: string; current_version_id?: string }[];
  }>(request, "evals.list", { limit: 50 });
  // The seeded eval, not "newest": authored evals persist and have no completed run.
  const row =
    page.items.find((item) => item.name.startsWith("bug-fix judge")) ?? page.items[0];
  return { id: row.id, versionId: row.current_version_id ?? "" };
}

/** The spec mutates the shared seed fixture; put the version back to six cases. */
async function removeImported(request: APIRequestContext, versionId: string) {
  const cases = await invoke<{ items: { id: string; input: string }[] }>(
    request,
    "eval_cases.list",
    { version_id: versionId, limit: 100 },
  );
  for (const testCase of cases.items) {
    if (testCase.input.startsWith("playwright ")) {
      await invoke(request, "eval_cases.delete", { id: testCase.id, version_id: versionId });
    }
  }
  const after = await invoke<{ items: { input: string }[] }>(request, "eval_cases.list", {
    version_id: versionId,
    limit: 100,
  });
  if (after.items.some((testCase) => testCase.input.startsWith("playwright "))) {
    throw new Error("the imported cases were not removed; the seeded count would drift");
  }
}

test("a run exports as a document and cases import into the version", async ({ page, request }) => {
  const { id, versionId } = await firstEval(request);
  try {
    await page.goto(`/evals/${id}/interop`);
    await expect(page.getByRole("button", { name: "Export", exact: true })).toBeVisible();

    // The seeded eval has a completed run; export writes a gzip JSONL document in S09.
    await page.getByRole("button", { name: "Export", exact: true }).click();
    await expect(page.locator(".callout", { hasText: "Document" })).toBeVisible();
    await expect(page.locator(".callout")).toContainText("results");

    // Import is the inverse: a JSON array of cases, all or nothing.
    await page
      .getByLabel("Cases (JSON)")
      .fill('[{"input":"playwright one"},{"input":"playwright two"}]');
    await page.getByRole("button", { name: "Import", exact: true }).click();
    await expect(page.getByText("Imported 2 cases.")).toBeVisible();
    await expect(page.getByText("2 cases imported into this version.")).toBeVisible();

    await expect(page.locator(".problem")).toHaveCount(0);
  } finally {
    // The seeded version's six cases are a baseline assertion in evals.spec.ts; a failed
    // assertion here must not leave the imported rows behind for the next spec.
    await removeImported(request, versionId);
  }
});

test("an invalid bundle is refused before it reaches the action", async ({ page, request }) => {
  const { id } = await firstEval(request);
  await page.goto(`/evals/${id}/interop`);
  await page.getByLabel("Cases (JSON)").fill("not json");
  await page.getByRole("button", { name: "Import", exact: true }).click();
  await expect(page.getByText("The bundle is not JSON")).toBeVisible();
});

test("the eval interop tab has no horizontal page scroll at 1280 and 390", async ({
  page,
  request,
}) => {
  const { id } = await firstEval(request);
  await page.goto(`/evals/${id}/interop`);
  await expect(page.getByRole("button", { name: "Export", exact: true })).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `width ${width}`).toBeLessThanOrEqual(0);
  }
});
