// The public design pack's programme boundary.
//
// The public site carries the programme (Portfolio, Projects, Plan, Validation, Execution,
// Documents, Contribute, System architecture). The V2 product-design study is the operator
// console's: signed out it is neither in the sidebar nor reachable by a direct hash, except
// through the explicit reviewer mode `?design=1`.
//
// Run: node docs/design/scripts/check-v2.cjs
"use strict";

const { chromium } = require("playwright-core");
const assert = require("node:assert/strict");
const path = require("node:path");
const os = require("node:os");
const { pathToFileURL } = require("node:url");

const base = pathToFileURL(path.resolve(__dirname, "../studio.html")).href;
const shot = (name) => path.join(process.env.CONVEYOR_QA_OUTPUT || os.tmpdir(), name);

(async () => {
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
    const page = await browser.newPage({ viewport: { width: 1440, height: 1060 } });
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
    const check = (value, label) => {
      assert(value, label);
      count++;
      console.log("PASS " + label);
    };
    const nav = async (name, query = "") => {
      await page.goto(`${base}${query}#${name}`);
      await page.reload();
      await page.waitForSelector("h1");
    };
    const action = async (name, filter = "") =>
      page.locator(`[data-v2="${name}"]${filter}`).first().click();
    const content = async () => page.locator("#main").innerText();
    const sidebar = async () => page.locator("#sidebar").innerText();
    const overflow = async () =>
      page.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1);

    // 1. The public default is Portfolio, not the design study's fiction.
    await page.goto(base);
    await page.waitForSelector("h1");
    check((await page.locator("h1").innerText()) === "Portfolio", "the default page is Portfolio");

    // 2. The public sidebar announces the programme only.
    await nav("portfolio");
    const side = await sidebar();
    check(side.includes("PROGRAMME"), "sidebar group PROGRAMME");
    for (const id of [
      "portfolio",
      "projects",
      "plan",
      "validation",
      "execution",
      "documents",
      "contribute",
      "architecture",
    ])
      check(
        (await page.locator(`#sidebar [data-page="${id}"]`).count()) === 1,
        "public sidebar destination " + id,
      );
    for (const id of [
      "overview",
      "organisation",
      "jobs",
      "procedures",
      "authoring",
      "improve",
      "operations",
      "toolkits",
      "metering",
      "evals",
      "knowledge",
      "jobtypes",
      "tools",
      "constraints",
    ])
      check(
        (await page.locator(`#sidebar [data-page="${id}"]`).count()) === 0,
        "design destination is not announced: " + id,
      );

    // 3. Deep links and titles for the public programme pages.
    for (const [id, title] of [
      ["portfolio", "Portfolio"],
      ["projects", "Projects"],
      ["plan", "Plan & WBS"],
      ["validation", "Validation"],
      ["execution", "Execution room"],
      ["documents", "Documents"],
      ["contribute", "Contribute"],
      ["architecture", "System architecture"],
    ]) {
      await nav(id);
      check((await page.locator("h1").innerText()) === title, `${id} renders its title`);
      check((await page.locator("#crumb").innerText()) === title, `${id} breadcrumb matches`);
    }

    // 4. A design-study hash is a sign-in gate publicly; ?design=1 is the reviewer's mode.
    await nav("organisation");
    check(
      (await content()).includes("part of the conveyor design study"),
      "a design destination is a sign-in gate publicly",
    );
    await nav("jobtypes");
    check(
      (await content()).includes("part of the conveyor design study"),
      "a legacy study page is a sign-in gate publicly",
    );
    for (const id of ["organisation", "jobs", "operations", "toolkits", "metering", "jobtypes"]) {
      await nav(id, "?design=1");
      check(
        (await page.locator("h1").count()) === 1,
        `the reviewer mode renders ${id} with one heading`,
      );
      check(
        !(await content()).includes("part of the conveyor design study"),
        `the reviewer mode is not the gate for ${id}`,
      );
    }

    // 5. Without a snapshot every public page is honest; the coverage pack is still real.
    await nav("portfolio");
    check(
      (await content()).includes("No published snapshot is loaded"),
      "the public portfolio is honest without a snapshot",
    );
    await nav("projects");
    check(
      (await content()).includes("No published snapshot is loaded"),
      "the public projects page is honest without a snapshot",
    );
    await nav("plan");
    check(
      (await content()).includes("No published snapshot is loaded"),
      "the public Boards tab is live-only and says so",
    );
    check(
      (await page.locator('[data-v2-input="plan-seed-project"]').count()) === 0,
      "no plan seed is loaded",
    );
    await nav("execution");
    check(
      (await content()).includes("No published snapshot is loaded"),
      "the public execution room is honest without a snapshot",
    );
    await nav("validation");
    const validation = await content();
    check(validation.includes("48"), "the Validation page carries the measured 48 projects");
    await nav("contribute");
    check(
      (await content()).includes("No ready unit is published right now"),
      "the public contribute page is honest without a registry",
    );

    // 6. No fictional enterprise programme text renders on any public page.
    for (const id of [
      "portfolio",
      "projects",
      "plan",
      "validation",
      "execution",
      "documents",
      "contribute",
      "architecture",
    ]) {
      await nav(id);
      const text = await content();
      for (const fiction of [
        "Connected public services",
        "Harvest identity baseline architecture",
        "Sample programme snapshot",
        "Citizen services",
      ])
        check(!text.includes(fiction), `${id} carries no fiction: ${fiction}`);
    }

    // 7. Documents reads the generated index, and platform outputs add their group.
    await nav("documents");
    check((await content()).includes("SPEC_V2"), "documents index lists SPEC_V2");
    check(
      (await page.locator('a[href="docs/SPEC_V2.html"]').count()) === 1,
      "documents index links SPEC_V2 as HTML",
    );
    await page.evaluate(() => {
      window.CONVEYOR_OUTPUTS = {
        schema: "design-outputs/v1",
        generated_at: "2026-11-13T00:00:00Z",
        outputs: [
          {
            id: "o-1",
            title: "Published note",
            project_id: "p-1",
            project_key: "S02",
            project_name: "Skeleton",
            path: "docs/output-o-1.html",
            updated_at: "2026-11-13T00:00:00Z",
          },
        ],
      };
    });
    await page.locator('#sidebar [data-page="documents"]').first().click();
    await page.waitForTimeout(200);
    check(
      (await page.locator("#main").textContent()).includes("Platform outputs"),
      "documents show the platform-outputs group",
    );
    check((await content()).includes("Published note"), "documents list the published output");
    check(
      (await page.locator('a[href="docs/output-o-1.html"]').count()) === 1,
      "the published output links its standalone page",
    );

    // 8. Responsive: every public destination at 1440, 1024 and 390.
    const destinations = [
      "portfolio",
      "projects",
      "plan",
      "validation",
      "execution",
      "documents",
      "contribute",
      "architecture",
    ];
    for (const width of [1440, 1024, 390]) {
      await page.setViewportSize({ width, height: 1060 });
      for (const id of destinations) {
        await nav(id);
        check(
          (await page.locator("h1").count()) === 1,
          `${id} renders one heading at ${width}px`,
        );
        check(!(await overflow()), `${id} has no page overflow at ${width}px`);
      }
    }

    // 9. Screenshots of the public page set.
    await page.setViewportSize({ width: 1440, height: 1060 });
    for (const id of destinations) {
      await nav(id);
      await page.evaluate(() => document.fonts.ready);
      await page.screenshot({ path: shot(`conveyor-v2-${id}.png`) });
    }

    check(errors.length === 0, "no browser runtime errors: " + errors.join(" | "));
    check(failed.length === 0, "no failed assets: " + failed.join(" | "));
    check(external.length === 0, "no external network dependencies: " + external.join(" | "));
    console.log(JSON.stringify({ checks: count, errors, failed, external }));
  } finally {
    await browser.close();
  }
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
