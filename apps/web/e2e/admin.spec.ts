import { expect, test, type Page } from "./fixture";

import { invoke } from "./helpers";

const baseURL = process.env.BASE_URL ?? "http://127.0.0.1:8931";

/** Swap the dev cookie. The file's storageState is alice; bob is the empty tenant. */
async function as(page: Page, login: string) {
  const host = new URL(baseURL).hostname;
  await page.context().addCookies([
    { name: "conveyor_dev", value: login, domain: host, path: "/" },
  ]);
}

test("the registry, its schema, the audit trail, the licence and the honest externals", async ({
  page,
  request,
}) => {
  // A real write first, so the audit tab has something this test owns.
  const title = `Admin audit probe ${Date.now()}`;
  await invoke(request, "requirements.create", { title });

  await page.goto("/settings/admin");
  await expect(page.getByRole("heading", { name: "Admin" })).toBeVisible();

  // The registry: actions.list rows, and actions.describe's own schema on click.
  await expect(page.locator("table.table tbody tr").first()).toBeVisible();
  await page.getByRole("button", { name: "requirements.list" }).click();
  await expect(page.getByRole("heading", { name: "Input schema" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Output schema" })).toBeVisible();

  // The audit log is append-only and tenant-scoped: the create above is in it.
  await page.getByRole("button", { name: "Audit", exact: true }).click();
  await expect(page.getByText("requirements.create").first()).toBeVisible();

  // The licence is the synced entitlement, read and never written here.
  await page.getByRole("button", { name: "Licence", exact: true }).click();
  await expect(page.locator("table", { hasText: "App" })).toContainText("conveyor");
  await expect(page.getByText("checked")).toBeVisible();

  // Workers is the conveyor's own inventory now: the durable queue and the host registry, not
  // an external service. The queue has the scheduled kinds in dev; no host is registered.
  await page.getByRole("button", { name: "Workers", exact: true }).click();
  const workers = page.locator("section.panel", {
    hasText: "The conveyor's own durable queue",
  });
  await expect(workers).toBeVisible();
  await expect(workers.getByRole("heading", { name: "Queue" })).toBeVisible();
  // A fresh seed may have no queued work yet; the panel must say so, never hide the section.
  await expect(
    workers.locator("table.table tbody tr").first().or(workers.getByText("No queued work.")),
  ).toBeVisible();
  await expect(page.getByText("No hosts are registered.")).toBeVisible();

  // Billing is the local ledger summary now (issue #450), not an external service.
  await page.getByRole("button", { name: "Billing", exact: true }).click();
  const billing = page.locator("section.panel", { hasText: "The ledger figure over its window" });
  await expect(billing).toBeVisible();
  await expect(billing).toContainText("Inference calls");
  await expect(billing).toContainText("Total");
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("an empty filter and an empty host registry say so", async ({ page }) => {
  await page.goto("/settings/admin");
  await expect(page.getByRole("heading", { name: "Admin" })).toBeVisible();

  // The filter's own empty line, never a bare table. The read is emptied at the seam because
  // this workflow is not empty: web.yml starts `conveyor worker`, the seed's maintenance
  // schedules queue `memory.consolidate`, and with no `strata_rag` credential here it exhausts
  // its attempts and reaches the terminal `dead` state (measured: 10 dead rows before this
  // spec runs). The property under test is the screen's — an empty page renders its empty
  // line — so the page is the thing injected, exactly as the audit read is for the error panel.
  await page.route("**/api/actions/jobs.list/invocations", (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ output: { items: [], next_cursor: null } }),
    }),
  );
  await page.getByRole("button", { name: "Jobs", exact: true }).click();
  const status = page.getByLabel("Status");
  await expect(status).toBeVisible();
  await status.selectOption("dead");
  await expect(page.getByText("No job matches this filter.")).toBeVisible();
  await expect(page.locator("table.table tbody tr")).toHaveCount(0);

  // The workers panel's host registry is a second, independently empty read — this one real:
  // nothing in this suite registers a host (`host_registry` holds no row).
  await page.getByRole("button", { name: "Workers", exact: true }).click();
  await expect(page.getByText("No hosts are registered.")).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("a tab whose action fails renders the screen's error panel", async ({ page }) => {
  // The admin screen is read-only apart from `jobs.retry`, so no input the UI offers can be
  // refused by the API. Its error state is the transport/bff failure path: `ErrorPanel`
  // (`role=alert`, `.panel.problem`) replaces the tab body. The 503 below is injected at the
  // network seam for that one call; every other request, and the boot, hit the real API.
  await page.route("**/api/actions/audit.list/invocations", (route) =>
    route.fulfill({
      status: 503,
      contentType: "application/problem+json",
      body: JSON.stringify({
        type: "internal",
        title: "Service Unavailable",
        status: 503,
        detail: "the audit store is unavailable",
      }),
    }),
  );

  await page.goto("/settings/admin");
  await expect(page.getByRole("heading", { name: "Admin" })).toBeVisible();
  await page.getByRole("button", { name: "Audit", exact: true }).click();
  await expect(page.getByRole("heading", { name: "That did not load" })).toBeVisible();
  await expect(page.locator(".panel.problem")).toContainText("the audit store is unavailable");
});

test("a second tenant reads its own manifest and its own audit trail", async ({ page }) => {
  // dev-b is a real tenant: the earlier specs wrote audit rows as bob, so the tab is a
  // tenant-scoped read rather than an empty page.
  await as(page, "bob");
  await page.goto("/settings/admin");
  await expect(page.getByRole("heading", { name: "Admin" })).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);

  await page.getByRole("button", { name: "Audit", exact: true }).click();
  await expect(page.locator("table.table").first()).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
});

test("the admin screen has no horizontal page scroll at 1280 and 390", async ({ page }) => {
  await page.goto("/settings/admin");
  await expect(page.getByRole("heading", { name: "Admin" })).toBeVisible();
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `width ${width}`).toBeLessThanOrEqual(0);
  }
});
