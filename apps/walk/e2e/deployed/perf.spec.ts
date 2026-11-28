import { expect, test } from "@playwright/test";

// The live performance budgets (S4, W7; GEMBA-WALK-SPEC §9). Measured on the deployed game host
// because the budget is about the shipped client, not the dev server:
//   - shell + entrance ≤ 250 KB gzip of JS/HTML/CSS, excluding the wasm engine;
//   - no frame over 50 ms after the first two;
//   - a five-minute soak within 1.5× the first-load heap (SOAK=1; skipped by default).
// Read-only. Run only through `npm run test:deployed`.

const SHELL_BUDGET_BYTES = 250 * 1024;
const FRAME_BUDGET_MS = 50;

async function signIn(page: any) {
  await page.goto("/");
  const gate = page.getByRole("button", { name: "Continue with Keycloak" });
  const today = page.getByRole("heading", { name: "Today" });
  await expect(gate.or(today)).toBeVisible({ timeout: 30_000 });
  if (await gate.isVisible()) await gate.click();
  await expect(today).toBeVisible({ timeout: 30_000 });
}

/** A cold load: the browser cache is disabled, so `transferSize` is the real encoded size. */
async function coldLoad(page: any, context: any) {
  const cdp = await context.newCDPSession(page);
  await cdp.send("Network.setCacheDisabled", { cacheDisabled: true });
  await page.reload();
  const gate = page.getByRole("button", { name: "Continue with Keycloak" });
  const today = page.getByRole("heading", { name: "Today" });
  await expect(gate.or(today)).toBeVisible({ timeout: 30_000 });
  if (await gate.isVisible()) await gate.click();
  await expect(today).toBeVisible({ timeout: 30_000 });
}

test("the shell and entrance stay within 250 KB gzip", async ({ page, context }) => {
  await signIn(page);
  await coldLoad(page, context);
  const measured = await page.evaluate(() => {
    const nav = performance.getEntriesByType("navigation")[0] as PerformanceNavigationTiming;
    const documentBytes = nav?.transferSize ?? 0;
    const resources = (performance.getEntriesByType("resource") as PerformanceResourceTiming[])
      .filter((entry) => /\.(js|css)$/.test(new URL(entry.name).pathname))
      .map((entry) => ({
        path: new URL(entry.name).pathname,
        // `transferSize` is 0 on a cache hit; `encodedBodySize` still carries the compressed size.
        bytes: entry.encodedBodySize || entry.transferSize || 0,
        cached: (entry.transferSize || 0) === 0,
      }));
    return {
      documentBytes,
      resources,
      total: resources.reduce((sum, entry) => sum + entry.bytes, documentBytes),
    };
  });
  test.info().annotations.push({
    type: "perf.shell",
    description: JSON.stringify(measured),
  });
  expect(
    measured.total,
    `shell bytes: ${measured.total} (document ${measured.documentBytes}, ${measured.resources.length} resources)`,
  ).toBeLessThanOrEqual(SHELL_BUDGET_BYTES);
});

test("no frame over 50 ms after the first two", async ({ page }) => {
  await signIn(page);
  const deltas = await page.evaluate(
    () =>
      new Promise<number[]>((resolve) => {
        const frames: number[] = [];
        let last = performance.now();
        const start = last;
        function step(now: number) {
          frames.push(now - last);
          last = now;
          if (now - start > 5_000) resolve(frames);
          else requestAnimationFrame(step);
        }
        requestAnimationFrame(step);
      }),
  );
  const after = deltas.slice(2);
  const sorted = [...after].sort((a, b) => a - b);
  const p95 = sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * 0.95))] ?? 0;
  const worst = Math.max(...after);
  test.info().annotations.push({
    type: "perf.frames",
    description: JSON.stringify({ frames: after.length, p95, worst }),
  });
  expect(worst, `frames p95=${p95.toFixed(1)}ms worst=${worst.toFixed(1)}ms`).toBeLessThanOrEqual(
    FRAME_BUDGET_MS,
  );
});

test("a five-minute soak stays within 1.5x the first-load heap", async ({ page, context }) => {
  test.skip(!process.env.SOAK, "set SOAK=1 to run the five-minute soak");
  test.setTimeout(8 * 60_000);
  await signIn(page);

  // `usedJSHeapSize` includes garbage until a collection runs; the budget is about RETAINED
  // memory, so each reading is taken after a forced GC (CDP HeapProfiler), not a raw sample.
  const cdp = await context.newCDPSession(page);
  await cdp.send("HeapProfiler.enable");
  const heap = async () => {
    await cdp.send("HeapProfiler.collectGarbage");
    return page.evaluate(() => (performance as any).memory?.usedJSHeapSize ?? 0);
  };
  const first = await heap();
  const routes = ["/", "/plan", "/programme", "/decisions", "/execution", "/organisation"];
  const deadline = Date.now() + 5 * 60_000;
  let visits = 0;
  while (Date.now() < deadline) {
    for (const route of routes) {
      await page.getByRole("button", { name: "Palette" }).click();
      const dialog = page.getByRole("dialog", { name: "Palette" });
      await dialog.getByRole("searchbox", { name: "Search" }).fill(route);
      await dialog.getByRole("button", { name: new RegExp(`^${route}\\s`) }).first().click();
      // The soak measures the heap across navigation and the stream; the full matrix is the
      // route-render proof. A screen that mounts is enough here, gated or live.
      await expect(page.locator("#screen > *").first()).toBeVisible({ timeout: 20_000 });
      visits += 1;
    }
    // The budget describes a user session, not a machine hammering the router: one pass of the
    // six rooms, then a deliberate pause. A stress rate (12 navigations/second) is recorded
    // separately in the evidence, where it is a finding rather than the acceptance protocol.
    await page.waitForTimeout(2_000);
  }
  const last = await heap();
  test.info().annotations.push({
    type: "perf.soak",
    description: JSON.stringify({ first, last, visits, routes }),
  });
  expect(last, `heap first=${first} last=${last} over ${visits} visits`).toBeLessThanOrEqual(
    Math.round(first * 1.5),
  );
});
