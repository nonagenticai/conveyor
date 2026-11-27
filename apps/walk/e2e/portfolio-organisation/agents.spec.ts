import { expect, test } from "@playwright/test";

// Agents: the capability-agent read, the table's columns, and the empty/loading/error/gate
// states. The dev seed has no capability agents, so the populated rendering uses a route
// double of the server's own AgentOut shape; the empty state is the seed's truth.

test.describe.configure({ mode: "serial" });

const AGENT = {
  key: "code-reviewer",
  status: "active",
  model_ref: "claude-opus-5-5",
  prompt_ref: "prompts/review.md",
  allowed_actions: ["repository_search", "diff.read"],
};

test("the agents render their key, model and allowed-action count", async ({ page }) => {
  await page.route("**/api/actions/capability_agents.list/invocations", (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ output: { items: [AGENT] } }),
    }),
  );
  await page.goto("/agents");
  await expect(page.getByRole("heading", { name: "Agents", level: 1 })).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);
  const row = page.getByRole("row", { name: /code-reviewer/ });
  await expect(row).toContainText("active");
  await expect(row).toContainText("claude-opus-5-5");
  await expect(row).toContainText("prompts/review.md");
  await expect(row).toContainText("2");
});

test("the empty state is the app's own message", async ({ page }) => {
  await page.goto("/agents");
  await expect(page.getByText("No capability agents in this tenant yet.", { exact: true })).toBeVisible();
});

test("the loading state is shown while the read is in flight", async ({ page }) => {
  await page.route("**/api/actions/capability_agents.list/invocations", async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 800));
    await route.continue();
  });
  await page.goto("/agents");
  await expect(page.getByText("Loading…", { exact: true })).toBeVisible();
});

test("a read failure is the problem panel", async ({ page }) => {
  await page.route("**/api/actions/capability_agents.list/invocations", (route) =>
    route.fulfill({
      status: 500,
      contentType: "application/problem+json",
      body: JSON.stringify({
        title: "Internal Server Error",
        status: 500,
        detail: "the agents read failed",
      }),
    }),
  );
  await page.goto("/agents");
  await expect(page.locator(".problem")).toContainText("the agents read failed");
});

test("a signed-out visit is the sign-in gate", async ({ page }) => {
  await page.goto("/agents");
  await page.evaluate(() => {
    document.cookie = "conveyor_dev=; Max-Age=0; Path=/";
  });
  await page.goto("/agents");
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
});
