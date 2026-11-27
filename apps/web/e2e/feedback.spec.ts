import { expect, test, type Page } from "./fixture";

const baseURL = process.env.BASE_URL ?? "http://127.0.0.1:8931";

/** Swap the dev cookie. The file's storageState is alice (admin of dev-a). */
async function as(page: Page, login: string) {
  const host = new URL(baseURL).hostname;
  await page.context().addCookies([
    { name: "conveyor_dev", value: login, domain: host, path: "/" },
  ]);
}

/** A per-run title keeps the dev database's leftover rows from making the count non-deterministic. */
function uniqueTitle(tag: string): string {
  return `feedback-e2e-${tag}-${Date.now()}`;
}

test("a tenant submits feedback and sees it with its status", async ({ page }) => {
  await page.goto("/feedback");
  await expect(page.getByRole("heading", { name: "Feedback", level: 1 })).toBeVisible();

  const title = uniqueTitle("submit");
  await page.getByLabel("Kind").selectOption("bug");
  await page.getByLabel("Title").fill(title);
  await page
    .getByLabel("What happened, or what would help?")
    .fill("The boards page loses a filter after a reload.");
  await page.getByRole("button", { name: "Send feedback" }).click();

  await expect(page.getByText("Thank you — the platform operator will see this.")).toBeVisible();
  const row = page.locator('[data-testid="feedback-row"]', { hasText: title });
  await expect(row).toHaveCount(1);
  await expect(row.locator(".pill")).toHaveText("open");
  await expect(row.getByText("/feedback")).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);

  // A reload reads the row back from the tenant's own list, not from component state.
  await page.reload();
  await expect(
    page.locator('[data-testid="feedback-row"]', { hasText: title }),
  ).toHaveCount(1);
});

test("an empty submit is refused inline and sends nothing", async ({ page }) => {
  await page.goto("/feedback");
  await page.getByRole("button", { name: "Send feedback" }).click();
  await expect(
    page.getByText("A title and a description are required."),
  ).toBeVisible();
  await expect(page.getByText("Thank you")).toHaveCount(0);
});

test("a tenant sees only its own feedback", async ({ page }) => {
  const title = uniqueTitle("isolation");
  await page.goto("/feedback");
  await page.getByLabel("Title").fill(title);
  await page.getByLabel("What happened, or what would help?").fill("Alice's private report.");
  await page.getByRole("button", { name: "Send feedback" }).click();
  await expect(
    page.locator('[data-testid="feedback-row"]', { hasText: title }),
  ).toHaveCount(1);

  // dev-b's Bob must not see dev-a's row: the screen lists the tenant's own rows, and the
  // action is tenant-scoped on the server.
  await as(page, "bob");
  await page.goto("/feedback");
  await expect(page.getByRole("heading", { name: "Feedback", level: 1 })).toBeVisible();
  await expect(
    page.locator('[data-testid="feedback-row"]', { hasText: title }),
  ).toHaveCount(0);
});

test("a signed-out visitor gets the sign-in gate, not the form", async ({ browser, baseURL }) => {
  const context = await browser.newContext({
    baseURL,
    storageState: { cookies: [], origins: [] },
  });
  const page = await context.newPage();
  await page.goto("/feedback");
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Send feedback" })).toHaveCount(0);
  await context.close();
});
