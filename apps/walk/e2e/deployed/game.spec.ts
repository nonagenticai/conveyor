import { expect, test } from "@playwright/test";

// The live proof: a real `conveyor-game` sign-in on https://game.nonagentic.ai/, the
// Portfolio read through the same-origin API, and the world on the game host. The token is
// memory-only, so navigation stays in the SPA (a hard load re-enters the gate).

test("the deployed game signs in through SSO and reads the Portfolio", async ({ page }) => {
  const me: number[] = [];
  const serverErrors: string[] = [];
  page.on("response", (response) => {
    if (response.url().includes("/api/me")) me.push(response.status());
    if (response.status() >= 500) serverErrors.push(`${response.status()} ${response.url()}`);
  });

  await page.goto("/");
  const gate = page.getByRole("button", { name: "Continue with Keycloak" });
  const today = page.getByRole("heading", { name: "Today" });
  await expect(gate.or(today)).toBeVisible({ timeout: 30_000 });
  if (await gate.isVisible()) await gate.click();
  await expect(today).toBeVisible({ timeout: 30_000 });
  await expect(page.locator(".problem")).toHaveCount(0);
  await expect(page.locator(".who")).not.toHaveText("");

  // The entrance is the world host's own client, and the world engine is the image's.
  await expect(page.locator("#world-canvas")).toHaveAttribute("data-engine", "ecs", {
    timeout: 20_000,
  });

  // In-SPA navigation: the token is memory-only.
  await page.getByRole("navigation", { name: "Rooms" }).getByRole("button", { name: "Portfolio & plan" }).click();
  await page.getByRole("dialog", { name: "Portfolio & plan" }).getByRole("button", { name: "Today" }).click();
  await expect(page.getByRole("heading", { name: "Today" })).toBeVisible();
  await expect(page.locator(".problem")).toHaveCount(0);

  expect(me).toContain(200);
  expect(serverErrors, `5xx responses: ${serverErrors.join(", ")}`).toEqual([]);
});
