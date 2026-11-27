import { expect, test } from "@playwright/test";

// The exemplar destination: the read, the empty state, a create with Idempotency-Key, an
// update with If-Match, and the server's refusal shown verbatim. Serial: the rows accumulate.

test.describe.configure({ mode: "serial" });

const unique = () => `Walk programme ${Date.now()}-${Math.floor(Math.random() * 1000)}`;

test("the Portfolio read renders with its empty or populated state", async ({ page }) => {
  await page.goto("/portfolio");
  await expect(page.getByRole("heading", { name: "Portfolio" })).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
  const empty = page.getByText("No programmes yet", { exact: false });
  if (await empty.isVisible()) {
    await expect(empty).toBeVisible();
  } else {
    await expect(page.getByRole("table")).toBeVisible();
  }
});

test("a create carries Idempotency-Key, refetches, and a duplicate is refused verbatim", async ({
  page,
}) => {
  const name = unique();
  await page.goto("/portfolio");
  await page.getByRole("button", { name: /New programme|Create the first programme/ }).first().click();
  const dialog = page.getByRole("dialog", { name: "New programme" });
  await expect(dialog).toBeVisible();
  const created = page.waitForRequest(
    (request) => request.url().includes("/api/actions/programmes.create/invocations"),
  );
  await dialog.getByLabel(/Name/).fill(name);
  await dialog.getByRole("button", { name: "Submit" }).click();
  const request = await created;
  expect(request.headers()["idempotency-key"]).toBeTruthy();
  await expect(page.getByRole("link", { name })).toBeVisible();

  // The same create again: the server's refusal is displayed, the sheet stays open.
  await page.getByRole("button", { name: "New programme" }).click();
  const duplicate = page.getByRole("dialog", { name: "New programme" });
  await duplicate.getByLabel(/Name/).fill(name);
  await duplicate.getByRole("button", { name: "Submit" }).click();
  const error = duplicate.locator(".form-error");
  await expect(error).not.toHaveText("");
  await expect(duplicate).toBeVisible();
  await duplicate.getByRole("button", { name: "Close" }).click();
});

test("an update carries If-Match and refetches the new name", async ({ page }) => {
  const name = unique();
  await page.goto("/portfolio");
  await page.getByRole("button", { name: /New programme|Create the first programme/ }).first().click();
  const create = page.getByRole("dialog", { name: "New programme" });
  await create.getByLabel(/Name/).fill(name);
  await create.getByRole("button", { name: "Submit" }).click();
  const row = page.getByRole("row", { name: new RegExp(name) });
  await expect(row).toBeVisible();

  await row.getByRole("button", { name: "Edit" }).click();
  const edit = page.getByRole("dialog", { name: new RegExp(`Edit ${name}`) });
  await expect(edit).toBeVisible();
  const updated = page.waitForRequest(
    (request) => request.url().includes("/api/actions/programmes.update/invocations"),
  );
  const renamed = `${name} v2`;
  await edit.getByLabel(/Name/).fill(renamed);
  await edit.getByRole("button", { name: "Submit" }).click();
  expect((await updated).headers()["if-match"]).toBeTruthy();
  await expect(page.getByRole("link", { name: renamed })).toBeVisible();
});

test("a signed-out 401 is a sign-in gate, not a blank screen", async ({ page }) => {
  await page.goto("/portfolio");
  await page.evaluate(() => {
    document.cookie = "conveyor_dev=; Max-Age=0; Path=/";
  });
  await page.goto("/portfolio");
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
});
