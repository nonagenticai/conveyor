import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

import { expect, test } from "@playwright/test";

// The wasm FrameView must equal the native golden (W1d/P6.1). The wasm package is built by
// the image and CI, never committed; without it this spec skips and the engine spec still
// checks the labelled fallback.
const pkg = resolve("pkg/walk_world.js");
const scenarioPath = resolve("../../docs/evidence/walk-frame-scenario.json");
const goldenPath = resolve("../../docs/evidence/walk-frame.sha256");

test.skip(!existsSync(pkg), "the wasm package is not built in this checkout");

test("the wasm frame-view hash equals the native golden", async ({ page }) => {
  const scenario = JSON.parse(readFileSync(scenarioPath, "utf8"));
  const golden = readFileSync(goldenPath, "utf8").trim();
  await page.goto("/");
  const hash = await page.evaluate(async (scenario) => {
    // @ts-expect-error the package is built by the image/CI
    const module = await import("/pkg/walk_world.js");
    await module.default();
    const world = new module.WasmWalkWorld(BigInt(scenario.log.seed), JSON.stringify(scenario.config));
    const last = Math.max(...scenario.log.entries.map((entry: { tick: number }) => entry.tick));
    for (let tick = 0; tick <= last; tick += 1) {
      for (const timed of scenario.log.entries.filter(
        (entry: { tick: number }) => entry.tick === tick,
      )) {
        if (timed.entry.Snapshot) world.push_snapshot(JSON.stringify(timed.entry.Snapshot));
        if (timed.entry.Input) world.push_input(JSON.stringify(timed.entry.Input));
      }
      world.advance(16.666667);
    }
    return world.view_hash();
  }, scenario);
  expect(hash).toBe(golden);
});
