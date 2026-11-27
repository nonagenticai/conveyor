import { expect, test, type APIRequestContext } from "@playwright/test";

// The Organisation destination: the eight reads, the three writes, the shell's loading state,
// a read failure as the problem panel, and the sign-in gate. Serial: the rows accumulate.

test.describe.configure({ mode: "serial" });

async function invoke(request: APIRequestContext, name: string, input: unknown): Promise<any> {
  const response = await request.post(`/api/actions/${name}/invocations`, { data: { input } });
  if (!response.ok()) {
    throw new Error(`${name} refused: ${response.status()} ${await response.text()}`);
  }
  const body = await response.json();
  return body && typeof body === "object" && "output" in body ? body.output : body;
}

test("the organisation renders the tenant's rows or the exact empty messages", async ({ page }) => {
  const stamp = Date.now();
  const industry = await invoke(page.request, "industries.create", {
    key: `s1_org_${stamp}`,
    name: `S1 industry ${stamp}`,
  });
  await invoke(page.request, "departments.create", {
    name: `S1 department ${stamp}`,
    industry_id: industry.id,
    ordinal: 1,
    is_primary: true,
  });

  await page.goto("/organisation");
  await expect(page.getByRole("heading", { name: "Organisation", level: 1 })).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
  await expect(page.getByTestId("organisation-industries")).toContainText(`S1 industry ${stamp}`);
  await expect(page.getByTestId("organisation-departments")).toContainText(
    `S1 department ${stamp}`,
  );

  // Every other family is either the server's rows or the app's exact empty message.
  for (const [testId, empty] of [
    ["organisation-jobs", "No catalogue positions in this tenant yet."],
    ["organisation-approval-policies", "No approval policies in this tenant yet."],
    ["organisation-decision-policies", "No decision policies in this tenant yet."],
    ["organisation-refinements", "No refinements in this tenant yet."],
    ["organisation-ladder", "No ladder decisions in this tenant yet."],
  ] as const) {
    await expect(page.getByTestId(testId).or(page.getByText(empty, { exact: true }))).toBeVisible();
  }
  await expect(page.getByText(/laya configuration row\(s\) readable/)).toBeVisible();
});

test("a department write carries Idempotency-Key and refetches", async ({ page }) => {
  const stamp = Date.now();
  const industry = await invoke(page.request, "industries.create", {
    key: `s1_write_${stamp}`,
    name: `S1 write industry ${stamp}`,
  });
  const name = `S1 written department ${stamp}`;

  await page.goto("/organisation");
  await page.getByRole("button", { name: "Create department" }).click();
  const dialog = page.getByRole("dialog", { name: "Create department" });
  await expect(dialog).toBeVisible();

  const sent = page.waitForRequest((request) =>
    request.url().includes("/api/actions/departments.create/invocations"),
  );
  await dialog.getByLabel(/^Name/).fill(name);
  await dialog.getByLabel(/^Industry id/).fill(industry.id);
  await dialog.getByLabel(/^Ordinal/).fill("1");
  await dialog.getByLabel(/^Is primary/).check();
  await dialog.getByRole("button", { name: "Submit" }).click();
  const invocation = await sent;
  expect(invocation.headers()["idempotency-key"]).toBeTruthy();

  await expect(page.getByTestId("organisation-departments")).toContainText(name);
});

test("an approval-policy refusal is the server's, verbatim", async ({ page }) => {
  await page.goto("/organisation");
  await page.getByRole("button", { name: "Set approval policy" }).click();
  const dialog = page.getByRole("dialog", { name: "Set approval policy" });
  await expect(dialog).toBeVisible();

  await dialog.getByLabel(/^Action/).fill("programmes.create");
  await dialog.getByLabel(/^Approval/).fill("required");
  await dialog.getByLabel(/^Self approval/).fill("forbidden");
  await dialog.getByRole("button", { name: "Submit" }).click();

  const refusal = dialog.locator(".form-error");
  await expect(refusal).toContainText("no approval to waive");
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Close" }).click();
});

test("the loading state is shown while a read is in flight", async ({ page }) => {
  await page.route("**/api/actions/industries.list/invocations", async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 800));
    await route.continue();
  });
  await page.goto("/organisation");
  await expect(page.getByText("Loading…", { exact: true })).toBeVisible();
});

test("a read failure is the problem panel, not a blank screen", async ({ page }) => {
  await page.route("**/api/actions/industries.list/invocations", (route) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({
        title: "Internal Server Error",
        status: 500,
        detail: "the organisation read failed",
      }),
    }),
  );
  await page.goto("/organisation");
  await expect(page.locator(".problem")).toContainText("the organisation read failed");
});

test("a signed-out visit is the sign-in gate", async ({ page }) => {
  await page.goto("/organisation");
  await page.evaluate(() => {
    document.cookie = "conveyor_dev=; Max-Age=0; Path=/";
  });
  await page.goto("/organisation");
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
});
