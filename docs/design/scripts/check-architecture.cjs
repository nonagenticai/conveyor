// System architecture page checks. Same optional local Playwright + Chrome setup as the other
// studio passes. The page is a local design study: no backend, no network, no runtime Mermaid.
const { chromium } = require("playwright-core");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const os = require("node:os");
const { pathToFileURL } = require("node:url");
const dir = path.resolve(__dirname, "..");
const base = pathToFileURL(path.join(dir, "studio.html")).href;
const shot = (name) =>
  path.join(process.env.CONVEYOR_QA_OUTPUT || os.tmpdir(), name);

(async () => {
  const diagramsDir = path.join(dir, "architecture", "diagrams");
  const sources = fs
    .readdirSync(diagramsDir)
    .filter((name) => name.endsWith(".mmd"))
    .sort();
  assert(sources.length > 0, "the architecture page has Mermaid sources");
  for (const name of sources) {
    const stem = name.replace(/\.mmd$/, "");
    for (const theme of ["light", "dark"]) {
      assert(
        fs.existsSync(path.join(diagramsDir, `${stem}.${theme}.svg`)),
        `${stem}.${theme}.svg exists`,
      );
    }
  }

  const browser = await chromium.launch({
    executablePath:
      process.env.CHROME_BIN ||
      (process.platform === "darwin"
        ? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
        : undefined),
    headless: true,
  });
  let count = 0;
  try {
    const page = await browser.newPage({
      viewport: { width: 1440, height: 1060 },
    });
    const errors = [],
      failed = [],
      external = [];
    page.on("pageerror", (e) => errors.push(e.message));
    page.on("requestfailed", (r) => {
      if (r.failure()?.errorText !== "net::ERR_ABORTED")
        failed.push(r.url() + ": " + r.failure()?.errorText);
    });
    page.on("request", (r) => {
      if (!/^(file:|data:|blob:)/.test(r.url())) external.push(r.url());
    });
    const check = (v, label) => {
      assert(v, label);
      count++;
      console.log("PASS " + label);
    };
    const nav = async (name) => {
      await page.goto(base + "#" + name);
      await page.reload();
      await page.waitForSelector("h1");
    };
    const tab = async (value) => {
      await page
        .locator(`.arch-tabs [data-arch-tab="${value}"]`)
        .first()
        .click();
      if (value !== "High level") await waitImages();
    };
    const content = async () => page.locator("#main").textContent();
    const waitImages = async () => {
      await page.waitForFunction(
        () => {
          const imgs = [...document.querySelectorAll("figure[data-diagram] img")];
          return (
            imgs.length > 0 &&
            imgs.every((img) => img.complete && img.naturalHeight > 0)
          );
        },
        null,
        { timeout: 15000 },
      );
      check(
        await page.$$eval("figure[data-diagram] img", (imgs) =>
          imgs.every((img) => img.complete && img.naturalHeight > 0),
        ),
        "every diagram image loaded",
      );
    };
    const overflow = async () =>
      page.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1);

    // 1. The destination itself.
    await nav("architecture");
    check((await page.locator("h1").innerText()) === "System architecture", "h1 is System architecture");
    check((await page.locator("#crumb").innerText()) === "System architecture", "breadcrumb matches");
    check(
      (await page.locator('#sidebar [data-page="architecture"]').count()) === 1,
      "sidebar has exactly one architecture destination",
    );
    check(
      (await content()).includes("V2 · PROPOSED"),
      "the page carries the V2 proposed ribbon",
    );
    check(
      (await content()).includes("ArchiMate 3.2"),
      "the page names ArchiMate 3.2",
    );
    for (const value of [
      "High level",
      "Runtime",
      "Data & knowledge",
      "Improvement & delivery",
      "UI map",
    ])
      check(
        (await page.locator(`.arch-tabs [data-arch-tab="${value}"]`).count()) ===
          1,
        "tab " + value,
      );
    check(
      (await page.locator('#main [data-arch-tab="UI map"]').count()) === 2,
      "the header UI map action and the tab are both present",
    );

    // 2. The Mermaid source embedded in the page must match the .mmd files (no drift).
    const embedded = await page.evaluate(() => window.ArchitecturePack.sources);
    check(
      Object.keys(embedded).length === sources.length,
      `the page embeds ${sources.length} Mermaid sources`,
    );
    for (const name of sources) {
      const key = name.replace(/\.mmd$/, "");
      const file = fs.readFileSync(path.join(diagramsDir, name), "utf8").trim();
      check(
        (embedded[key] || "").trim() === file,
        `${key} source matches its .mmd`,
      );
    }

    // 3. High level: three ArchiMate views and the notation legend.
    await tab("High level");
    check(
      (await page.locator(".arch-archimate svg.arch-svg").count()) === 3,
      "three ArchiMate views",
    );
    for (const view of ["org", "app", "tech"])
      check(
        (await page.locator(`[data-view="${view}"] svg.arch-svg`).count()) === 1,
        "ArchiMate view " + view,
      );
    check(
      (await page.locator('.arch-el[data-status="V2"]').count()) > 10,
      "the views distinguish proposed V2 elements",
    );
    check(
      (await page.locator('.arch-el[data-status="V1"]').count()) > 10,
      "the views distinguish implemented V1 elements",
    );
    check(
      (await page.locator(".arch-legend").count()) === 1,
      "the ArchiMate legend is present",
    );
    const high = await content();
    for (const phrase of [
      "Job release",
      "Job binding",
      "Decision ladder",
      "Tool broker",
      "conveyor Service :8930",
      "Postgres 18",
    ])
      check(high.includes(phrase), "high-level view names " + phrase);

    // 4. Runtime: the five low-level diagrams.
    await tab("Runtime");
    check(
      (await page.locator('figure[data-diagram]').count()) === 5,
      "five runtime diagrams",
    );
    for (const key of [
      "context",
      "run-sequence",
      "broker-sequence",
      "run-state",
      "decision-ladder",
    ])
      check(
        (await page.locator(`figure[data-diagram="${key}"]`).count()) === 1,
        "runtime diagram " + key,
      );
    const runtime = await content();
    for (const phrase of [
      "409 job_binding_stale",
      "409 tool_not_ready",
      "403 tool_effect_forbidden",
      "422 tool_input_invalid",
      "429 tool_budget_exceeded",
      "L2",
    ])
      check(runtime.includes(phrase), "runtime content names " + phrase);

    // 5. Data & knowledge: three ER diagrams plus the two pipelines.
    await tab("Data & knowledge");
    check(
      (await page.locator('figure[data-diagram]').count()) === 5,
      "five data and knowledge diagrams",
    );
    for (const key of [
      "er-organisation-work",
      "er-execution-decisions",
      "er-knowledge-tools",
      "knowledge-pipeline",
      "memory-lifecycle",
    ])
      check(
        (await page.locator(`figure[data-diagram="${key}"]`).count()) === 1,
        "data diagram " + key,
      );
    check(
      (await content()).includes("NEW IN V2"),
      "the new-versus-reused table is present",
    );

    // 6. Improvement and delivery.
    await tab("Improvement & delivery");
    check(
      (await page.locator('figure[data-diagram]').count()) === 2,
      "two improvement and delivery diagrams",
    );
    const improvement = await content();
    for (const phrase of ["TOGAF ADM", "SAFe", "PMP", "T0", "T17"])
      check(improvement.includes(phrase), "improvement content names " + phrase);

    // 7. The UI map covers every sidebar destination.
    await tab("UI map");
    check(
      (await page.locator('figure[data-diagram="ui-map"]').count()) === 1,
      "the UI map diagram is present",
    );
    const sidebarIds = await page.$$eval("#sidebar [data-page]", (els) =>
      [...new Set(els.map((el) => el.dataset.page))],
    );
    const mappedIds = await page.$$eval("[data-ui-page]", (els) =>
      els.map((el) => el.dataset.uiPage),
    );
    check(sidebarIds.length >= 30, "the sidebar has at least 30 destinations");
    for (const id of sidebarIds)
      check(mappedIds.includes(id), "UI map covers sidebar destination " + id);
    check(
      (await page.locator("[data-admin-surface]").count()) === 4,
      "the four tenant-administration surfaces are listed",
    );
    const map = await content();
    check(map.includes("Providers & credentials"), "the admin table names providers");
    check(map.includes("Model tiers"), "the admin table names model tiers");
    check(
      (await page.locator(".arch-ui-table tbody tr").count()) >= 35,
      "the UI map table has at least 35 rows",
    );

    // 8. Responsive: no page-level overflow on any tab at three widths.
    for (const width of [1440, 1024, 390]) {
      await page.setViewportSize({ width, height: 1000 });
      for (const value of [
        "High level",
        "Runtime",
        "Data & knowledge",
        "Improvement & delivery",
        "UI map",
      ]) {
        await tab(value);
        check(!(await overflow()), `${value} fits at ${width}`);
      }
    }

    // 9. Dark theme swaps the pre-rendered variants and leaves the ArchiMate views readable.
    await page.setViewportSize({ width: 1440, height: 1060 });
    await tab("Runtime");
    await page.evaluate(() =>
      document.documentElement.setAttribute("data-theme", "dark"),
    );
    check(
      (await page.evaluate(
        () =>
          getComputedStyle(
            document.querySelector('figure[data-diagram="context"] .arch-dark'),
          ).display,
      )) !== "none",
      "dark theme shows the dark diagram variant",
    );
    check(
      (await page.evaluate(
        () =>
          getComputedStyle(
            document.querySelector('figure[data-diagram="context"] .arch-light'),
          ).display,
      )) === "none",
      "dark theme hides the light diagram variant",
    );
    await page.evaluate(() =>
      document.documentElement.removeAttribute("data-theme"),
    );
    await page.evaluate(() =>
      document.documentElement.setAttribute("data-theme", "dark"),
    );
    await nav("architecture");
    await tab("High level");
    check(
      (await page.locator(".arch-svg").count()) === 3,
      "ArchiMate views render in dark theme",
    );
    await page.evaluate(() =>
      document.documentElement.removeAttribute("data-theme"),
    );

    // 10. No runtime errors, failed assets or external requests.
    check(errors.length === 0, "no runtime errors");
    check(failed.length === 0, "no failed local assets");
    check(external.length === 0, "no external page requests");

    // 11. Screenshots.
    await nav("architecture");
    await tab("High level");
    await page.evaluate(() => document.fonts.ready);
    await page.screenshot({ path: shot("conveyor-architecture-high.png") });
    await tab("UI map");
    await page.screenshot({ path: shot("conveyor-architecture-ui-map.png") });
    await page.setViewportSize({ width: 390, height: 1000 });
    await tab("Runtime");
    await page.screenshot({ path: shot("conveyor-architecture-mobile.png") });
    await page.setViewportSize({ width: 1440, height: 1060 });
    await nav("architecture");
    await tab("Data & knowledge");
    await page.evaluate(() =>
      document.documentElement.setAttribute("data-theme", "dark"),
    );
    await page.screenshot({ path: shot("conveyor-architecture-dark.png") });
    await page.evaluate(() =>
      document.documentElement.removeAttribute("data-theme"),
    );

    console.log(`Completed ${count} system architecture checks.`);
  } finally {
    await browser.close();
  }
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
