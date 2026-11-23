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
    // #468: on a publish branch `data/snapshot.js` is the live pack; this check asserts the
    // design fixtures, so it asks the app for the no-snapshot shape before any page script.
    await page.addInitScript(() => {
      window.CONVEYOR_QA_NO_SNAPSHOT = true;
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
      "outputs",
      "plan",
      "validation",
      "procedures",
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
      "programme",
      "adm",
    ])
      check(
        (await page.locator(`#sidebar [data-page="${id}"]`).count()) === 0,
        "design destination is not announced: " + id,
      );

    // 3. Deep links and titles for the public programme pages.
    for (const [id, title] of [
      ["portfolio", "Portfolio"],
      ["projects", "Projects"],
      ["outputs", "Outputs"],
      ["plan", "Plan & WBS"],
      ["validation", "Validation"],
      ["procedures", "Procedures"],
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
    for (const id of [
      "organisation",
      "jobs",
      "authoring",
      "improve",
      "operations",
      "toolkits",
      "metering",
      "jobtypes",
      "programme",
      "adm",
    ]) {
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

    // 4a. The programme-definition destination carries the MSP layer end to end.
    await nav("programme", "?design=1");
    check((await content()).includes("Definition set"), "the programme definition set renders");
    await page
      .locator('[data-v2="tab"][data-value="Benefits map & profiles"]')
      .first()
      .click();
    check((await content()).includes("Benefits map"), "the benefits map renders");
    await page
      .locator('[data-v2="tab"][data-value="Tranches & reviews"]')
      .first()
      .click();
    check((await content()).includes("Review · tranche 0"), "the tranche review renders");
    await page
      .locator('[data-v2="tab"][data-value="Benefits map & profiles"]')
      .first()
      .click();
    await page.locator('[data-v2="benefit"][data-id="BEN-03"]').first().click();
    await page
      .locator('[data-v2="tab"][data-value="Benefits realization"]')
      .first()
      .click();
    check(
      (await content()).includes("not measured"),
      "a benefit without a measured baseline is visible",
    );
    await page.locator('[data-v2="transition-record"]').first().click();
    check(
      (await content()).includes("benefit_baseline_missing"),
      "a transition without a baseline is refused",
    );
    for (const width of [1440, 1024, 390]) {
      await page.setViewportSize({ width, height: 1000 });
      await nav("programme", "?design=1");
      check(
        (await page.locator("h1").count()) === 1,
        `programme definition renders at ${width}`,
      );
      check(!(await overflow()), `programme definition has no page overflow at ${width}`);
    }
    await page.setViewportSize({ width: 1440, height: 1060 });
    await nav("programme");
    check(
      (await content()).includes("part of the conveyor design study"),
      "the programme destination is a sign-in gate publicly",
    );

    // 4b. The architecture-capability destination carries the TOGAF ADM layer end to end.
    await nav("adm", "?design=1");
    check((await content()).includes("Principles catalog"), "the principles catalog renders");
    check((await content()).includes("Tailored ADM profile"), "the tailored ADM profile renders");
    await page.locator('[data-v2="tab"][data-value="ADM phases"]').first().click();
    check((await content()).includes("Phase rail"), "the ADM phase rail renders");
    check((await content()).includes("Contracts across the ADM"), "the four contract kinds render");
    await page.locator('[data-v2="adm-advance"]').first().click();
    check(
      (await content()).includes("phase_artifacts_missing"),
      "a phase with a draft mandatory artefact is refused",
    );
    await page.locator('[data-v2="tab"][data-value="Artefact repository"]').first().click();
    await page.locator('[data-v2="adm-artifact"][data-id="ART-005"]').first().click();
    check((await content()).includes("Requirements catalog"), "a catalog opens with its entries");
    check((await content()).includes("REQ-011"), "the catalog's entries are rows");
    await page.locator('[data-v2="adm-artifact"][data-id="ART-003"]').first().click();
    check((await content()).includes("Stakeholder map"), "a matrix opens with its axes");
    await page.locator('[data-v2="adm-approve"]').first().click();
    check(
      (await content()).includes("a new version supersedes"),
      "an approval supersedes the version",
    );
    await page.locator('[data-v2="tab"][data-value="Board & compliance"]').first().click();
    check((await content()).includes("Compliance reviews"), "the compliance register renders");
    await page.locator('[data-v2="adm-waiver"]').first().click();
    check(
      (await content()).includes("waiver_owner_required"),
      "a waiver without an owner and date is refused",
    );
    await page.locator('[data-v2="adm-h-change"]').first().click();
    check((await content()).includes("Change request recorded"), "phase H records a change");
    await page.locator('[data-v2="adm-h-kata"]').first().click();
    check(
      (await content()).includes("phase_h_outcome_exists"),
      "phase H records exactly one outcome per review",
    );
    for (const width of [1440, 1024, 390]) {
      await page.setViewportSize({ width, height: 1000 });
      await nav("adm", "?design=1");
      check((await page.locator("h1").count()) === 1, `architecture capability renders at ${width}`);
      check(!(await overflow()), `architecture capability has no page overflow at ${width}`);
    }
    await page.setViewportSize({ width: 1440, height: 1060 });
    await nav("adm");
    check(
      (await content()).includes("part of the conveyor design study"),
      "the architecture capability is a sign-in gate publicly",
    );

    // 5. Without a snapshot every public page is honest; the coverage pack is still real.
    await nav("portfolio");
    check(
      (await content()).includes("No published snapshot is loaded"),
      "the public portfolio is honest without a snapshot",
    );
    // #528: a timeline is never drawn from a fixture — no snapshot, no `.gantt`, and no MSP/TOGAF
    // artefact rail either (the rails read the snapshot's rows, never the study fixtures).
    check(
      (await page.locator("#main .gantt").count()) === 0,
      "the portfolio timeline does not render without a snapshot",
    );
    check(
      (await page.locator("#main .delivery-rail").count()) === 0,
      "no tranche or benefit rail renders without a snapshot",
    );
    check(
      (await page.locator("#main .delivery-scorecard").count()) === 0,
      "no standards scorecard renders without a snapshot",
    );
    // The generated standards record is published beside the snapshot; without the snapshot the
    // view has no subject to attach it to and says nothing, rather than listing artefacts.
    check(
      (await page.locator("#main .std-rail").count()) === 0,
      "no standards artefact rail renders without a snapshot",
    );
    await nav("projects");
    check(
      (await content()).includes("No published snapshot is loaded"),
      "the public projects page is honest without a snapshot",
    );
    check(
      (await page.locator("#main .kanban").count()) === 0,
      "the project board does not render without a snapshot",
    );
    check(
      (await page.locator("#main .kanban-rework").count()) === 0,
      "the rework lane does not render without a snapshot",
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
    await nav("procedures");
    check(
      (await content()).includes("not published on this pack"),
      "the public Procedures page is honest without a snapshot",
    );
    await nav("validation");
    const validation = await content();
    const measuredTotal = require(
      path.resolve(__dirname, "../data/coverage.json"),
    ).summary.projects.total;
    check(
      validation.includes(String(measuredTotal)),
      `the Validation page carries the measured ${measuredTotal} projects`,
    );
    await nav("contribute");
    check(
      (await content()).includes("No ready unit is published right now"),
      "the public contribute page is honest without a registry",
    );
    await nav("outputs");
    check(
      (await content()).includes("No output is published yet"),
      "the public outputs page is honest without a published row",
    );

    // 6. No fictional enterprise programme text renders on any public page.
    for (const id of [
      "portfolio",
      "projects",
      "outputs",
      "plan",
      "validation",
      "procedures",
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
    await page.locator('#sidebar [data-page="outputs"]').first().click();
    await page.waitForTimeout(200);
    check(
      (await content()).includes("Published note"),
      "the public Outputs page lists the published index",
    );
    check(
      (await page.locator('a[href="docs/output-o-1.html"]').count()) === 1,
      "the public Outputs page links the output's page",
    );

    // 8. Responsive: every public destination at 1440, 1024 and 390.
    const destinations = [
      "portfolio",
      "projects",
      "outputs",
      "plan",
      "validation",
      "procedures",
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

    // 9b. On a live snapshot every public page carries the feedback affordance, and the data
    // panel names only files the window actually serves. The snapshot is injected before the
    // page scripts so `live` is true without an HTTP server or a real publish.
    await page.addInitScript(() => {
      // #468: this section wants the live shape; the design-fixture flag above must stand down.
      window.CONVEYOR_QA_NO_SNAPSHOT = false;
      Object.defineProperty(window, "CONVEYOR_SNAPSHOT", {
        configurable: true,
        set() {},
        get() {
          return {
            schema: "design-snapshot/v1",
            tenant: "nonagentic",
            generated_at: "2026-11-16T00:00:00Z",
            git_sha: "a".repeat(40),
            payload_sha256: "b".repeat(64),
            programme: { feedback: [], projects: [] },
            work: { boards: [], lists: [], cards: [] },
            organisation: {},
            measure: {
              inference: {
                by_caller: [{ caller_kind: "agent_run", calls: 7, total_tokens: 900 }],
                by_model: [
                  {
                    model: "stub-model",
                    calls: 7,
                    prompt_tokens: 600,
                    completion_tokens: 300,
                    total_tokens: 900,
                    cache_hits: 2,
                  },
                ],
                totals: {
                  calls: 7,
                  prompt_tokens: 600,
                  completion_tokens: 300,
                  total_tokens: 900,
                  cache_hit_calls: 2,
                  cache_miss_calls: 5,
                  cache_write_calls: 0,
                  unknown_cache_calls: 0,
                  non_ok_calls: 0,
                },
              },
              note: "tokens are stored; cost is derived by the pinned price version",
            },
          };
        },
      });
    });
    for (const id of destinations) {
      await nav(id);
      const text = await content();
      check(text.includes("Live data & feedback"), `${id} carries the live data and feedback panel`);
      check(
        (await page
          .locator('#main a[href="https://github.com/nonagenticai/conveyor/discussions"]')
          .count()) >= 1,
        `${id} links Discussions`,
      );
      check(
        (await page
          .locator('#main a[href*="/issues/new?labels=feedback"]')
          .count()) === 1,
        `${id} links the prefilled report`,
      );
      check(!text.includes("data/manifest.json"), `${id} carries no dead data link`);
      check(!text.includes("data/programme.json"), `${id} carries no dead programme link`);
    }

    // #470: a live snapshot must not make the signed-out sidebar announce the
    // Programme-definition design destination. The public site carries the programme pages; the
    // V2 design study's own sidebar (signed in, or `?design=1`) is the only one that names it.
    await nav("portfolio");
    check(
      (await page.locator('#sidebar [data-page="programme"]').count()) === 0,
      "a live snapshot does not announce the programme destination publicly",
    );

    // 9c. Metering reads the live measurement (#458). It is a design destination — `?design=1`
    // is the reviewer's mode the public visitor never gets — so the live shape is exercised here
    // with the snapshot injected above: the by-caller/by-model rows and totals come from the
    // payload, no fixture figure survives, and the snapshot's own cost note is shown.
    await nav("metering", "?design=1");
    const metered = await page.locator("#main").innerText();
    check(metered.includes("agent_run"), "metering renders the live caller rows");
    check(metered.includes("stub-model"), "metering renders the live model rows");
    check(metered.includes("Cache hits"), "metering renders the cache-hit total");
    check(metered.includes("5 misses"), "the cache figures are the payload's");
    check(
      !metered.includes("18.4M") && !metered.includes("$412.80"),
      "metering shows no fixture number",
    );
    check(
      metered.includes("cost is derived by the pinned price version"),
      "metering carries the snapshot's cost note",
    );

    // 9d. A snapshot without `measure` says so; it never borrows a fixture roll-up.
    await page.addInitScript(() => {
      Object.defineProperty(window, "CONVEYOR_SNAPSHOT", {
        configurable: true,
        set() {},
        get() {
          return {
            schema: "design-snapshot/v1",
            tenant: "nonagentic",
            generated_at: "2026-11-16T00:00:00Z",
            git_sha: "a".repeat(40),
            payload_sha256: "b".repeat(64),
            programme: { feedback: [], projects: [] },
            work: { boards: [], lists: [], cards: [] },
            organisation: {},
            measure: {},
          };
        },
      });
    });
    await nav("metering", "?design=1");
    const unmetered = await page.locator("#main").innerText();
    check(
      unmetered.includes("No measurement published"),
      "a snapshot without measure.inference says the measurement is not published",
    );
    check(
      !unmetered.includes("18.4M") && !unmetered.includes("$412.80"),
      "the no-measure page shows no fixture number",
    );

    // 10. The delivery view's positive path: a published snapshot carrying work in progress and
    // completed work, a two-tranche programme, a project board with cards, a work package with
    // child cards, and priced / unpriced-only spend. Every fixture id that a route uses is a
    // UUID, because the page validates route ids before lookup.
    const LIVE_PROG = "11111111-1111-4111-8111-111111111111";
    const DONE_PROG = "22222222-2222-4222-8222-222222222222";
    const P_T1 = "33333333-3333-4333-8333-333333333331";
    const P_T2 = "33333333-3333-4333-8333-333333333332";
    const P_DONE_A = "44444444-4444-4444-8444-444444444441";
    const P_DONE_B = "44444444-4444-4444-8444-444444444442";
    const P_LONE = "55555555-5555-4555-8555-555555555551";
    const P_LONE_DONE = "66666666-6666-4666-8666-666666666661";
    const P_LONE_QUIET = "77777777-7777-4777-8777-777777777771";
    const CARD_WP = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa1";
    const CARD_CHILD_1 = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa2";
    const CARD_CHILD_2 = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa3";
    const CARD_LOOSE = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa4";
    const CARD_LONE = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbb1";
    const CARD_LONE_DONE = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbb2";
    const UNKNOWN = "99999999-9999-4999-8999-999999999999";
    const UNKNOWN_CARD = "99999999-9999-4999-8999-999999999998";
    await page.addInitScript(() => {
      window.CONVEYOR_QA_NO_SNAPSHOT = false;
      Object.defineProperty(window, "CONVEYOR_SNAPSHOT", {
        configurable: true,
        set() {},
        get() {
          return {
            schema: "design-snapshot/v1",
            tenant: "nonagentic",
            generated_at: "2026-11-20T12:00:00Z",
            git_sha: "a".repeat(40),
            payload_sha256: "b".repeat(64),
            programme: {
              programme: [
                { id: "11111111-1111-4111-8111-111111111111", name: "Live programme", status: "active" },
                { id: "22222222-2222-4222-8222-222222222222", name: "Closed programme", status: "closed" },
              ],
              projects: [
                { id: "33333333-3333-4333-8333-333333333331", key: "T1", name: "Tranche one project", programme_id: "11111111-1111-4111-8111-111111111111", status: "active", meta: { tranche: 1, issues: [], depends_on: [] } },
                { id: "33333333-3333-4333-8333-333333333332", key: "T2", name: "Tranche two project", programme_id: "11111111-1111-4111-8111-111111111111", status: "active", meta: { tranche: 2, issues: [], depends_on: [] } },
                { id: "44444444-4444-4444-8444-444444444441", key: "DA", name: "Closed programme project A", programme_id: "22222222-2222-4222-8222-222222222222", status: "closed", meta: { tranche: 1, issues: [], depends_on: [] } },
                { id: "44444444-4444-4444-8444-444444444442", key: "DB", name: "Closed programme project B", programme_id: "22222222-2222-4222-8222-222222222222", status: "closed", meta: { tranche: 2, issues: [], depends_on: [] } },
                { id: "55555555-5555-4555-8555-555555555551", key: "LONE", name: "Standalone live initiative", programme_id: null, status: "active", meta: { issues: [], depends_on: [] } },
                { id: "66666666-6666-4666-8666-666666666661", key: "DONE-LONE", name: "Standalone completed initiative", programme_id: null, status: "closed", meta: { issues: [], depends_on: [] } },
                { id: "77777777-7777-4777-8777-777777777771", key: "QUIET-LONE", name: "Standalone quiet completed initiative", programme_id: null, status: "closed", meta: { issues: [], depends_on: [] } },
              ],
            },
            work: {
              boards: [
                { id: "board-t1", project_id: "33333333-3333-4333-8333-333333333331" },
                { id: "board-lone", project_id: "55555555-5555-4555-8555-555555555551" },
                { id: "board-lone-done", project_id: "66666666-6666-4666-8666-666666666661" },
                { id: "board-lone-quiet", project_id: "77777777-7777-4777-8777-777777777771" },
              ],
              lists: [
                { id: "lane-t1-todo", board_id: "board-t1", name: "To do", kind: "active", position: 1 },
                { id: "lane-t1-done", board_id: "board-t1", name: "Done", kind: "done", position: 2 },
                { id: "lane-lone-todo", board_id: "board-lone", name: "To do", kind: "active", position: 1 },
                { id: "lane-lone-done-todo", board_id: "board-lone-done", name: "To do", kind: "active", position: 1 },
                { id: "lane-lone-quiet-todo", board_id: "board-lone-quiet", name: "To do", kind: "active", position: 1 },
              ],
              cards: [
                { id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa1", board_id: "board-t1", list_id: "lane-t1-todo", name: "Work package alpha", card_type_id: "ct-delivery", position: 1, created_at: "2026-11-01T00:00:00Z", updated_at: "2026-11-20T00:00:00Z" },
                { id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa2", board_id: "board-t1", list_id: "lane-t1-todo", parent_card_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa1", name: "Child one", card_type_id: "ct-delivery", position: 2, created_at: "2026-11-02T00:00:00Z", updated_at: "2026-11-10T00:00:00Z" },
                { id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa3", board_id: "board-t1", list_id: "lane-t1-todo", parent_card_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa1", name: "Child two", card_type_id: "ct-delivery", position: 3, created_at: "2026-11-03T00:00:00Z", updated_at: "2026-11-12T00:00:00Z", archived_at: "2026-11-12T00:00:00Z" },
                { id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa4", board_id: "board-t1", list_id: "lane-t1-todo", name: "Loose card", card_type_id: "ct-delivery", position: 4, created_at: "2026-11-04T00:00:00Z", updated_at: "2026-11-15T00:00:00Z" },
                { id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbb1", board_id: "board-lone", list_id: "lane-lone-todo", name: "Standalone card", card_type_id: "ct-delivery", position: 1, created_at: "2026-11-02T00:00:00Z", updated_at: "2026-11-19T00:00:00Z" },
                { id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbb2", board_id: "board-lone-done", list_id: "lane-lone-done-todo", name: "Completed initiative card", card_type_id: "ct-delivery", position: 1, created_at: "2026-06-01T00:00:00Z", updated_at: "2026-07-01T00:00:00Z" },
              ],
              card_types: [
                { id: "ct-delivery", code: "delivery_task", label: "Delivery task", definition: { acceptance: ["accept one"] } },
              ],
              project_activity: [
                { project_id: "33333333-3333-4333-8333-333333333331", started_at: "2026-11-01T00:00:00Z", last_activity_at: "2026-11-20T00:00:00Z" },
                { project_id: "33333333-3333-4333-8333-333333333332", started_at: "2026-11-05T00:00:00Z", last_activity_at: "2026-11-18T00:00:00Z" },
                { project_id: "44444444-4444-4444-8444-444444444441", started_at: "2026-08-01T00:00:00Z", last_activity_at: "2026-09-15T00:00:00Z" },
                { project_id: "44444444-4444-4444-8444-444444444442", started_at: "2026-08-10T00:00:00Z", last_activity_at: "2026-09-30T00:00:00Z" },
                { project_id: "55555555-5555-4555-8555-555555555551", started_at: "2026-11-02T00:00:00Z", last_activity_at: "2026-11-19T00:00:00Z" },
                { project_id: "66666666-6666-4666-8666-666666666661", started_at: "2026-06-01T00:00:00Z", last_activity_at: "2026-07-01T00:00:00Z" },
              ],
              runs: [],
              run_steps: [],
              approvals: [],
            },
            measure: {
              inference: {
                by_project: [
                  { project_id: "33333333-3333-4333-8333-333333333331", total_tokens: 1000000, cost_usd: "0.100000", priced_calls: 1, unpriced_calls: 0 },
                  { project_id: "44444444-4444-4444-8444-444444444441", total_tokens: 1500000, cost_usd: "0.420000", priced_calls: 2, unpriced_calls: 1 },
                  { project_id: "44444444-4444-4444-8444-444444444442", total_tokens: 500000, cost_usd: "0.080000", priced_calls: 1, unpriced_calls: 0 },
                  { project_id: "55555555-5555-4555-8555-555555555551", total_tokens: 250000, cost_usd: "0.035000", priced_calls: 1, unpriced_calls: 0 },
                  { project_id: "66666666-6666-4666-8666-666666666661", total_tokens: 42000, cost_usd: "0.000000", priced_calls: 0, unpriced_calls: 1 },
                ],
                by_card: [
                  { card_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa1", total_tokens: 960000, cost_usd: "0.270000", priced_calls: 1, unpriced_calls: 0 },
                  { card_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa2", total_tokens: 1000, cost_usd: "0.001000", priced_calls: 1, unpriced_calls: 0 },
                  { card_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa4", total_tokens: 5000, cost_usd: "0.000000", priced_calls: 0, unpriced_calls: 1 },
                ],
              },
            },
            msp: {
              tranches: [
                { id: "tr-1", programme_id: "11111111-1111-4111-8111-111111111111", number: 1, state: "closed" },
                { id: "tr-2", programme_id: "11111111-1111-4111-8111-111111111111", number: 2, state: "active" },
              ],
              benefit_profiles: [],
              benefit_links: [],
              benefit_transitions: [],
              stakeholder_profiles: [],
              cadences: [],
              wbs_nodes: [
                { id: "wbs-1", project_id: "33333333-3333-4333-8333-333333333331", kind: "work_package", title: "Ship the delivery view", status: "open", acceptance: ["renders"] },
              ],
              tranche_reviews: [],
              register_items: [],
              project_baselines: [],
            },
          };
        },
      });
    });

    // 10a. L1: in-flight first, completed history below it, with the observed span and spend.
    await page.setViewportSize({ width: 1440, height: 1060 });
    await nav("portfolio");
    const l1 = await content();
    const historyText = await page.locator("#main .gantt-history").textContent();
    check(l1.includes("Delivery timeline · portfolio"), "L1 renders the portfolio timeline");
    check(l1.includes("Live programme"), "L1 lists the in-flight programme");
    check(l1.includes("Standalone live initiative"), "L1 lists the in-flight standalone initiative");
    check(
      (await page.locator("#main .gantt").count()) === 2,
      "L1 draws the in-flight Gantt and one completed-history Gantt",
    );
    check(
      (await page.locator("#main .gantt-history").count()) === 1,
      "the completed section is its own Gantt",
    );
    check(
      (await page.locator("#main .gantt-history .gantt-head-label").first().textContent()) ===
        "Completed · history",
      "the completed section is labelled as history",
    );
    const sectionOrder = await page.evaluate(() =>
      [...document.querySelectorAll("#main .gantt")].map((el) =>
        el.classList.contains("gantt-history") ? "history" : "live",
      ),
    );
    check(sectionOrder.join(",") === "live,history", "in-flight work renders before completed history");
    check(
      (await page.locator('#main .gantt-history .gantt-row[data-state="completed"]').count()) === 2,
      "both completed L1 rows carry the completed state",
    );
    check(
      (await page.locator("#main .gantt-history .gantt-state.completed").count()) === 2,
      "each completed row states completed in visible text",
    );
    check(historyText.includes("Closed programme"), "L1 lists the completed programme");
    check(
      historyText.includes("Standalone completed initiative"),
      "L1 lists the completed standalone project",
    );
    const ganttText = (await page.locator("#main .gantt").allTextContents()).join("\n");
    check(
      !ganttText.includes("Standalone quiet completed initiative"),
      "a completed row with no observed activity stays absent from the timeline",
    );
    check(
      historyText.includes("1 Aug 2026 → 30 Sep 2026"),
      "the completed programme shows its observed span",
    );
    check(
      historyText.includes("2.0M tok · $0.50*"),
      "a priced completed row shows tokens and money, marked unpriced",
    );
    check(
      historyText.includes("42.0k tok · —*"),
      "an unpriced-only completed row shows —*, never a dollar figure",
    );
    check(!l1.includes("$0.00"), "no L1 row prices an unpriced call as $0.00");

    // 10b. L2: a programme with two tranches.
    await nav(`portfolio/${LIVE_PROG}`);
    const l2 = await content();
    check(l2.includes("Delivery timeline · programme"), "the programme deep link opens L2");
    check(
      l2.includes("Tranche one project") && l2.includes("Tranche two project"),
      "L2 renders both of the programme's projects",
    );
    check(
      /TRANCHES? 1 · CLOSED/i.test(l2) && /TRANCHES? 2 · ACTIVE/i.test(l2),
      "a two-tranche programme groups its projects under both tranches",
    );
    check(
      l2.includes("T1 closed") && l2.includes("T2 active"),
      "the tranche rail carries both tranche states",
    );
    const l2Rows = await page.locator("#main .gantt-row .gantt-name").allInnerTexts();
    check(
      l2Rows.includes("Tranche one project") && l2Rows.includes("Tranche two project"),
      "L2's timeline carries both of the programme's projects",
    );
    check(
      !l2Rows.includes("Standalone live initiative"),
      "L2's timeline lists only this programme's projects",
    );

    // 10c. L3: the project kanban with a work package and a loose card.
    await nav(`projects/${P_T1}`);
    const l3 = await content();
    check(l3.includes("T1 · Tranche one project"), "the project deep link opens that project");
    check(l3.includes("Work package alpha"), "L3 renders the work-package card");
    check(l3.includes("WORK PACKAGE · 2"), "the work-package tile carries its child count");
    check(l3.includes("Loose card"), "L3 renders the loose card");
    check(
      (await page.locator("#main .kanban-col").count()) >= 2,
      "L3 renders its board lanes as columns",
    );
    check(l3.includes("961.0k tok · $0.27"), "a priced card shows tokens and money");
    check(l3.includes("5.0k tok · —*"), "an unpriced-only card shows —*, never $0.00");
    check(!l3.includes("$0.00"), "the project board never prices an unpriced call as $0.00");
    check(
      (await page
        .locator(`#main a.kanban-open[href="#projects/${P_T1}/wp/${CARD_WP}"]`)
        .count()) === 1,
      "the work-package card links to its L4 route",
    );

    // 10d. L4: the work package's own board with its child cards.
    await nav(`projects/${P_T1}/wp/${CARD_WP}`);
    const l4 = await content();
    check(l4.includes("T1 · Work package"), "the card deep link opens the work-package level");
    check(l4.includes("Work package alpha"), "L4 names the work package");
    check(l4.includes("Child one"), "L4 renders the open child card");
    check(l4.includes("1/2 cards done"), "L4 shows the child-card progress");
    check(l4.includes("No run is recorded"), "L4's empty execution is honest");
    check(
      (await page.locator('#main .kanban-col[data-lane="archived"] .kanban-card').count()) === 1,
      "the archived child renders once, in the archived column",
    );

    // 10e. Deep links: an unknown id must not silently fabricate a level. The programme id falls
    // back to the named portfolio level; the project id shows a real project's identity, never
    // the unknown id; an unknown card id is refused in words rather than rendered as a board.
    await nav(`portfolio/${UNKNOWN}`);
    const badProgramme = await content();
    check(
      badProgramme.includes("Delivery timeline · portfolio"),
      "an unknown programme id lands on the named portfolio level",
    );
    check(
      !badProgramme.includes("Delivery timeline · programme"),
      "an unknown programme id never fabricates a programme drill",
    );
    await nav(`projects/${UNKNOWN}`);
    const badProject = await content();
    check(!badProject.includes(UNKNOWN), "an unknown project id is never rendered as a project");
    check(
      badProject.includes("T1 · Tranche one project"),
      "an unknown project id falls back to a real project's named identity",
    );
    check(
      !badProject.includes("· Work package"),
      "an unknown project id does not fabricate a work-package level",
    );
    await nav(`projects/${P_T1}/wp/${UNKNOWN_CARD}`);
    const badCard = await content();
    check(
      badCard.includes("This card is not on this project's boards in the published snapshot."),
      "an unknown card id is refused explicitly, not silently rendered",
    );
    check(
      (await page.locator("#main .kanban-col").count()) === 0,
      "the refused card renders no board columns",
    );


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
