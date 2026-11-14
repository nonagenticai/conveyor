// Offline HTML interaction and responsive checks. Requires playwright-core and Chrome.
const { chromium } = require("playwright-core");
const assert = require("node:assert/strict");
const path = require("node:path");
const os = require("node:os");
const { pathToFileURL } = require("node:url");
const base = pathToFileURL(path.resolve(__dirname, "../studio.html")).href;
const shot = (name) =>
  path.join(process.env.CONVEYOR_QA_OUTPUT || os.tmpdir(), name);

(async () => {
  const browser = await chromium.launch({
    executablePath:
      process.env.CHROME_BIN ||
      (process.platform === "darwin"
        ? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
        : undefined),
    headless: true,
  });
  const page = await browser.newPage({
    viewport: { width: 1440, height: 1000 },
    deviceScaleFactor: 1,
  });
  const errors = [],
    external = [],
    failed = [];
  page.on("pageerror", (e) => errors.push(e.stack));
  page.on("request", (r) => {
    if (!/^(file:|blob:|data:|http:\/\/127\.0\.0\.1:18942)/.test(r.url()))
      external.push(r.url());
  });
  page.on("requestfailed", (r) => {
    if (r.failure()?.errorText !== "net::ERR_ABORTED") failed.push(r.url());
  });
  let checks = 0;
  const ok = (v, m) => {
    assert(v, m);
    checks++;
    console.log("PASS " + m);
  };
  const nav = async (id) => {
    await page.goto(base + "#" + id);
    await page.reload();
    await page.waitForSelector("h1");
  };
  const action = async (a, s = "") =>
    page.locator(`[data-e="${a}"]${s}`).first().click();
  const old = async (a, s = "") =>
    ((await page.locator("#dialog[open]").count())
      ? page.locator("#dialog")
      : page
    )
      .locator(`[data-action="${a}"]${s}`)
      .first()
      .click();
  const close = async () => {
    if (await page.locator("#dialog[open]").count()) await old("close-dialog");
  };
  const has = async (t) =>
    (await page.locator("#main").innerText()).includes(t);
  await nav("overview");
  await page.evaluate(() => document.fonts.ready);
  ok(
    (await page.locator(".e-programme").count()) === 3,
    "three concurrent programmes",
  );
  ok(
    (await page.locator(".e-table tbody tr").count()) === 6,
    "six projects in selected programme",
  );
  ok(await has("4,706"), "programme-scale work roll-up");
  await action("select-project", '[data-id="data"]');
  ok(
    (await page.locator(".e-focus h2").first().innerText()).includes(
      "Common data",
    ),
    "project focus updates",
  );
  await action("programme", '[data-id="resilience"]');
  ok(
    (await page.locator(".e-table tbody tr").count()) === 3,
    "programme switch changes project scope",
  );
  await action("programme", '[data-id="civic"]');
  await action("programme-tab", '[data-value="Integrated plan"]');
  ok(
    (await page.locator(".e-plan-row").count()) === 6,
    "integrated plan scoped to projects",
  );
  await action("programme-tab", '[data-value="Gate readiness"]');
  ok(
    (await page.locator(".e-gate-list > button").count()) === 6,
    "six governance gates",
  );
  // #70: the Projects destination is the live/snapshot programme surface, not the V1 fixture.
  // A clean checkout has no snapshot, so the honest state is the empty prompt; the console gate
  // (`check-console.cjs`) covers the signed-in project page and every write.
  await nav("projects");
  ok(
    await has("No published snapshot is loaded"),
    "projects is the live/snapshot destination, not a fixture",
  );
  await nav("work");
  ok(
    (await page.locator(".e-wbs-project").count()) === 6,
    "expandable project WBS hierarchy",
  );
  await action("wbs-toggle", '[data-id="identity"]');
  ok(
    (await page.locator(".e-wbs-package").count()) === 0,
    "WBS branch collapses",
  );
  await action("wbs-toggle", '[data-id="identity"]');
  ok(
    (await page.locator(".e-wbs-package").count()) === 2,
    "WBS drill-down reveals package contracts",
  );
  ok(
    (await page.locator(".e-table tbody tr").count()) === 6,
    "focused work-package detail records",
  );
  await page.locator("#e-work-filter").selectOption("identity");
  ok(
    (await page.locator(".e-table tbody tr").count()) === 2,
    "work filter scopes by project",
  );
  await action("work-detail", '[data-id="WP-2048"]');
  ok(
    (await page.locator("#dialog").innerText()).includes("WBS 1.2.3"),
    "work-package contract detail",
  );
  await action("work-run");
  ok(await has("Execution room"), "work opens recorded execution");
  await action("run-tab", '[data-value="Context & knowledge"]');
  ok(
    await has("Select → fetch → render → budget"),
    "compiled context pipeline visible",
  );
  await action("context-manifest");
  ok(
    (await page.locator("#dialog").innerText()).includes(
      "conveyor:work:WP-2048",
    ),
    "source manifest scoped to card",
  );
  await close();
  await action("run-tab", '[data-value="Permitted tools"]');
  ok(await has("knowledge.query"), "explicit knowledge grants visible");
  await action("run-tab", '[data-value="Evidence"]');
  ok(
    await has("Observer evidence"),
    "producer / observer / human chain visible",
  );
  await action("run-tab", '[data-value="Activity"]');
  await page
    .locator("#e-note")
    .fill("Check requirement realisation before the next gate.");
  await page.locator("#e-operator-note button").click();
  ok(
    await has("Operator guidance recorded"),
    "operator note appended to execution",
  );
  await action("run-review");
  await action("run-decision", '[data-value="Accepted"]');
  ok(
    (await page.locator("#form-error").innerText()).includes("reason"),
    "human decision requires reason",
  );
  await page
    .locator("#e-review-reason")
    .fill(
      "Accept baseline inventory; target architecture remains held pending traces.",
    );
  await action("run-decision", '[data-value="Accepted"]');
  ok(await has("Completed"), "baseline review records completion");
  ok(await has("Accept baseline inventory"), "decision reason retained");
  await nav("assurance");
  ok(await has("Requirement coverage"), "requirements traceability matrix");
  await action("trace-node", '[data-index="2"]');
  ok(await has("Requirements"), "trace opens governed asset CRUD");
  await nav("assurance");
  await action("assurance-tab", '[data-value="Architecture cycle"]');
  ok(
    (await page.locator(".e-cycle > button").count()) === 9,
    "architecture cycle phases and continuous requirements",
  );
  await action("adm-phase");
  await old("navigate", '[data-page="models"]');
  ok(
    (await page.locator("#dialog[open]").count()) === 0,
    "dialog navigation closes obsolete modal",
  );
  ok(
    await has("Architecture & UML"),
    "architecture repository remains available",
  );
  await nav("assurance");
  await action("assurance-tab", '[data-value="Control registers"]');
  await action("control-record");
  ok(
    (await page.locator("#dialog").innerText()).includes("Accountable"),
    "register inspection carries ownership",
  );
  await close();
  await action("assurance-tab", '[data-value="Baselines & changes"]');
  await action("change");
  await action("change-approve");
  ok(
    (await page.locator("#form-error").innerText()).includes("rationale"),
    "baseline approval requires rationale",
  );
  await page
    .locator("#e-change-reason")
    .fill("Approved bounded contract review with retained acceptance gates.");
  await action("change-approve");
  ok(await has("B4"), "approval produces next baseline");
  await old("navigate", '[data-page="execution"]');
  ok(
    (await page.locator(".e-run-pins").innerText()).includes("Baseline B3"),
    "in-flight run retains original baseline pin",
  );
  await nav("constraints");
  ok(
    (await page.locator(".e-toc-node").count()) === 4,
    "TOC causal nodes inspectable",
  );
  await action("toc-node", '[data-id="2"]');
  ok(
    (await page.locator("aside").last().innerText()).includes(
      "Contributing cause",
    ),
    "TOC selection changes evidence inspector",
  );
  for (const t of ["Future reality", "Prerequisite tree", "Critical chain"]) {
    await action("toc-tab", `[data-value="${t}"]`);
    ok(
      await has(
        t === "Critical chain" ? "Integrated delivery plan" : t + " tree",
      ),
      "TOC " + t + " view",
    );
  }
  await action("toc-injection");
  await page.locator('button[form="e-injection-form"]').click();
  await old("navigate", '[data-page="assurance"]');
  await action("assurance-tab", '[data-value="Control registers"]');
  ok(
    await has("Review delegation contract as an independent slice"),
    "injection recorded as reviewable proposal",
  );
  await nav("jobtypes");
  ok(await has("98 job types"), "historical seeds plus tenant extensions");
  await page.locator("#e-job-search").fill("rewrite");
  ok(
    (await page.locator("#e-job-results").innerText())
      .toLowerCase()
      .includes("rewrite"),
    "original historical codes searchable",
  );
  await page.locator("#e-job-search").fill("architecture_baseline_harvest");
  await action("job-select");
  ok(await has("Repository linked"), "source job entry contract retained");
  await action("edit-job", '[data-new="true"]');
  await page.locator('#e-job-form [name="name"]').fill("Complexity assurance");
  await page.locator('#e-job-form [name="code"]').fill("complexity_assurance");
  await page.locator('#e-job-form [name="process"]').selectOption("model");
  await page.locator('#e-job-form [name="agent"]').selectOption("architect");
  await page
    .locator('#e-job-form [name="description"]')
    .fill("Evaluate cross-project architecture evidence.");
  await page
    .locator('#e-job-form [name="inputs"]')
    .fill("approved_scope\nevidence_refs");
  await page.locator('#e-job-form [name="outputs"]').fill("assurance_report");
  await page
    .locator('#e-job-form [name="entry"]')
    .fill("Approved scope and available evidence");
  await page
    .locator('#e-job-form [name="exit"]')
    .fill("Report reviewed and accepted");
  await page.locator('button[form="e-job-form"]').click();
  ok(await has("Complexity assurance"), "custom job draft created");
  await action("publish-job");
  ok(
    (await page.locator(".e-job-definition").innerText()).includes("Published"),
    "bound job publishes after contract validation",
  );
  await action("instantiate-job");
  await page.locator('button[form="e-work-form"]').click();
  ok(
    await has("Complexity assurance"),
    "published job instantiated as work package",
  );
  await nav("jobtypes");
  await page.locator("#e-job-search").fill("architecture_baseline_harvest");
  await action("job-select");
  await action("edit-job", ":not([data-new])");
  await page
    .locator('#e-job-form [name="name"]')
    .fill("Changed future harvest role");
  await page.locator('button[form="e-job-form"]').click();
  await old("navigate", '[data-page="execution"]');
  ok(
    !(await page.locator(".e-run-strip").innerText()).includes(
      "Changed future",
    ),
    "active execution retains published job pin after draft edit",
  );
  await nav("knowledge");
  ok(
    (await page.locator(".e-knowledge-map > button").count()) === 4,
    "four distinct knowledge and memory categories",
  );
  ok(
    (await page.locator(".e-ingest-stages > div").count()) === 7,
    "seven upstream ingestion stages",
  );
  await action("source-select", '[data-index="1"]');
  await action("ingest-step");
  ok(await has("ING-029"), "source execution state inspectable");
  await action("new-source");
  await page.locator('#e-source-form [name="name"]').fill("Test domain source");
  await page
    .locator('#e-source-form [name="source"]')
    .fill("test-domain-source");
  await page
    .locator('#e-source-form [name="origin"]')
    .fill("Architecture Office");
  await page.locator('button[form="e-source-form"]').click();
  ok(await has("Test domain source"), "governed source added locally");
  for (let i = 0; i < 7; i++) await action("ingest-step");
  ok(
    (
      await page.locator(".e-source-list > button").first().innerText()
    ).includes("Delivered"),
    "sample source passes full ingestion pipeline",
  );
  await action("knowledge-tab", '[data-value="Retrieval & citations"]');
  await page.locator("#e-retrieval-form button").click();
  ok(
    (await page.locator(".e-citation").count()) === 3,
    "scoped retrieval returns cited sample evidence",
  );
  await action("citation", '[data-index="1"]');
  ok(
    (await page.locator("#dialog").innerText()).includes("Supersedes"),
    "tenant layer provenance and supersession inspectable",
  );
  await close();
  await action("pack-outage");
  await page.locator("#e-retrieval-form button").click();
  ok(
    await has("knowledge_degraded"),
    "missing required pin produces explicit retrieval failure",
  );
  await old("navigate", '[data-page="execution"]');
  await action("start-run");
  ok(
    await has("capability_not_ready"),
    "missing required pack refuses run before execution",
  );
  await old("navigate", '[data-page="knowledge"]');
  await action("pack-outage");
  await action("knowledge-tab", '[data-value="Packs & bindings"]');
  await action("pack-review");
  await page
    .locator("#e-pack-reason")
    .fill("Provenance, rights and supersession reviewed.");
  await action("pack-publish");
  ok(await has("capability@18"), "new immutable pack version published");
  ok(
    await has("capability@17 · immutable execution input"),
    "old pin remains after publication",
  );
  await action("knowledge-tab", '[data-value="Work-package memory"]');
  await page
    .locator('#e-memory-form [name="body"]')
    .fill("A reviewed contract slice resolves the delegation uncertainty.");
  await page.locator("#e-memory-form button").click();
  ok(
    await has("resolves the delegation uncertainty"),
    "hot work-package memory writes locally",
  );
  await action("consolidate");
  ok(await has("Unconsolidated rows"), "cold consolidation retains hot record");
  await action("knowledge-tab", '[data-value="Context compiler"]');
  ok(
    await has("Hot memory wins"),
    "context compiler preserves trust and freshness semantics",
  );
  await nav("models");
  ok(
    await has("Connected services"),
    "programme-scale architecture model available",
  );
  await nav("tools");
  await page.locator("#tool-search").fill("execInWorkspaceAsync");
  ok(
    await has("exec_in_workspace_async"),
    "coding API and original n8n options retained",
  );
  await nav("overview");
  await old("search");
  await page.locator("#global-search").fill("Knowledge");
  ok(
    (await page.locator("#search-results").innerText()).includes(
      "Knowledge & memory",
    ),
    "global search includes new destinations",
  );
  await close();
  const pages = [
    "overview",
    "projects",
    "work",
    "execution",
    "decisions",
    "assurance",
    "constraints",
    "requirements",
    "principles",
    "models",
    "types",
    "capabilities",
    "jobtypes",
    "processes",
    "agents",
    "tools",
    "knowledge",
    "evals",
    "runs",
    "usage",
  ];
  for (const width of [1440, 1024, 390]) {
    await page.setViewportSize({ width, height: 1000 });
    for (const id of pages) {
      await nav(id);
      ok((await page.locator("h1").count()) === 1, `${id} renders at ${width}`);
      ok(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth,
        ),
        `${id} has no page overflow at ${width}`,
      );
    }
  }
  for (const width of [1440, 1024, 390]) {
    await page.setViewportSize({ width, height: 1000 });
    for (const [id, actionName, values] of [
      ["overview", "programme-tab", ["Integrated plan", "Gate readiness"]],
      [
        "assurance",
        "assurance-tab",
        ["Architecture cycle", "Control registers", "Baselines & changes"],
      ],
      [
        "knowledge",
        "knowledge-tab",
        [
          "Packs & bindings",
          "Retrieval & citations",
          "Work-package memory",
          "Context compiler",
        ],
      ],
      [
        "execution",
        "run-tab",
        ["Evidence", "Context & knowledge", "Permitted tools"],
      ],
      [
        "constraints",
        "toc-tab",
        ["Future reality", "Prerequisite tree", "Critical chain"],
      ],
    ]) {
      await nav(id);
      for (const value of values) {
        await action(actionName, `[data-value="${value}"]`);
        ok(
          await page.evaluate(
            () => document.documentElement.scrollWidth <= innerWidth,
          ),
          `${id} / ${value} fits at ${width}`,
        );
      }
    }
  }
  await nav("overview");
  await page.locator(".mobile-menu").click();
  ok(
    await page
      .locator("body")
      .evaluate((n) => n.classList.contains("nav-open")),
    "mobile menu opens",
  );
  await page.keyboard.press("Escape");
  ok(
    !(await page
      .locator("body")
      .evaluate((n) => n.classList.contains("nav-open"))),
    "Escape dismisses mobile navigation",
  );
  await page.screenshot({
    path: shot("conveyor-enterprise-mobile.png"),
    fullPage: true,
  });
  await page.setViewportSize({ width: 1600, height: 1100 });
  for (const id of [
    "overview",
    "knowledge",
    "execution",
    "jobtypes",
    "assurance",
    "constraints",
  ]) {
    await nav(id);
    await page.evaluate(() => document.fonts.ready);
    await page.screenshot({
      path: shot("conveyor-enterprise-" + id + ".png"),
      fullPage: true,
    });
  }
  await old("theme");
  ok(
    (await page.locator("html").getAttribute("data-theme")) === "dark",
    "dark theme remains available",
  );
  await page.screenshot({
    path: shot("conveyor-enterprise-dark.png"),
    fullPage: true,
  });
  ok(errors.length === 0, "no browser runtime errors: " + errors.join("; "));
  ok(failed.length === 0, "no failed assets: " + failed.join("; "));
  ok(external.length === 0, "no external network dependencies");
  console.log(JSON.stringify({ checks, errors, failed, external }));
  await browser.close();
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
