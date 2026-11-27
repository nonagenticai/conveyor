import { expect, test, type APIRequestContext } from "@playwright/test";

// Project types: the request read, the draft → approved promotion, the server's refusal when
// the id names no draft, and the empty/loading/error/gate states. Serial: rows accumulate.

test.describe.configure({ mode: "serial" });

async function invoke(request: APIRequestContext, name: string, input: unknown): Promise<any> {
  const response = await request.post(`/api/actions/${name}/invocations`, { data: { input } });
  if (!response.ok()) {
    throw new Error(`${name} refused: ${response.status()} ${await response.text()}`);
  }
  const body = await response.json();
  return body && typeof body === "object" && "output" in body ? body.output : body;
}

async function request(page: any, stamp: number): Promise<string> {
  const externalRef = `s1-request-${stamp}`;
  await invoke(page.request, "project_requests.create", {
    external_ref: externalRef,
    external_origin: "walk-e2e",
  });
  return externalRef;
}

test("the request renders with its draft status", async ({ page }) => {
  const stamp = Date.now();
  const externalRef = await request(page, stamp);

  await page.goto("/project-types");
  await expect(page.getByRole("heading", { name: "Project types", level: 1 })).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
  const row = page.getByRole("row", { name: new RegExp(externalRef) });
  await expect(row).toContainText("draft");
  await expect(row.getByRole("button", { name: "Approve" })).toBeVisible();
});

test("an approve promotes the request and refetches", async ({ page }) => {
  const stamp = Date.now();
  const externalRef = await request(page, stamp);

  await page.goto("/project-types");
  const row = page.getByRole("row", { name: new RegExp(externalRef) });
  await row.getByRole("button", { name: "Approve" }).click();
  const dialog = page.getByRole("dialog", { name: "Approve project request" });
  await expect(dialog).toBeVisible();

  const sent = page.waitForRequest((request) =>
    request.url().includes("/api/actions/project_requests.approve/invocations"),
  );
  await dialog.getByRole("button", { name: "Submit" }).click();
  await sent;

  await expect(row).toContainText("approved");
  await expect(row.getByRole("button", { name: "Approve" })).toHaveCount(0);
});

test("an approve of an unknown id is the server's refusal, verbatim", async ({ page }) => {
  const stamp = Date.now();
  const externalRef = await request(page, stamp);

  await page.goto("/project-types");
  await page
    .getByRole("row", { name: new RegExp(externalRef) })
    .getByRole("button", { name: "Approve" })
    .click();
  const dialog = page.getByRole("dialog", { name: "Approve project request" });
  await expect(dialog).toBeVisible();

  await dialog.getByLabel(/^Id/).fill("00000000-0000-4000-8000-000000000000");
  await dialog.getByRole("button", { name: "Submit" }).click();

  const refusal = dialog.locator(".form-error");
  await expect(refusal).not.toHaveText("");
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Close" }).click();
});

test("the empty state is the app's own message", async ({ page }) => {
  await page.route("**/api/actions/project_requests.list/invocations", (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ output: [] }),
    }),
  );
  await page.goto("/project-types");
  await expect(page.getByText("No project request in this tenant yet.", { exact: true })).toBeVisible();
});

test("the loading state is shown while the read is in flight", async ({ page }) => {
  await page.route("**/api/actions/project_requests.list/invocations", async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 800));
    await route.continue();
  });
  await page.goto("/project-types");
  await expect(page.getByText("Loading…", { exact: true })).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route("**/api/actions/project_requests.list/invocations", (route) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({
        title: "Internal Server Error",
        status: 500,
        detail: "the request read failed",
      }),
    }),
  );
  await page.goto("/project-types");
  await expect(page.locator(".problem")).toContainText("the request read failed");
});

test("a signed-out visit is the sign-in gate", async ({ page }) => {
  await page.goto("/project-types");
  await page.evaluate(() => {
    document.cookie = "conveyor_dev=; Max-Age=0; Path=/";
  });
  await page.goto("/project-types");
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
});
