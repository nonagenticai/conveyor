// SPEC_V2 interaction and responsive checks. Same optional local Playwright + Chrome
// dependencies as check-studio.cjs / check-learning.cjs. All records are local fixtures.
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
    const action = async (name, filter = "") =>
      page.locator(`[data-v2="${name}"]${filter}`).first().click();
    const tab = async (key, value) =>
      action("tab", `[data-key="${key}"][data-value="${value}"]`);
    const content = async () => page.locator("#main").innerText();
    const toastText = async () => {
      await page.waitForTimeout(60);
      return page.locator("#toast").innerText();
    };
    const overflow = async () =>
      page.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1);
    const sidebar = async () => page.locator("#sidebar").innerText();

    // 1. The V2 sidebar and all new destinations.
    await nav("portfolio");
    const side = await sidebar();
    for (const group of [
      "PORTFOLIO & PLAN",
      "ORGANISATION",
      "PROCEDURES & AUTHORING",
      "GOVERN & DESIGN",
      "KNOWLEDGE & MEMORY",
      "MEASURE & IMPROVE",
      "OPERATIONS",
    ])
      check(side.includes(group), "sidebar group " + group);
    for (const id of [
      "portfolio",
      "plan",
      "organisation",
      "jobs",
      "procedures",
      "authoring",
      "improve",
      "operations",
      "toolkits",
      "metering",
      "architecture",
      "documents",
    ])
      check(
        (await page.locator(`#sidebar [data-page="${id}"]`).count()) === 1,
        "sidebar destination " + id,
      );

    // 2. Deep links and titles.
    for (const [id, title] of [
      ["portfolio", "Portfolio"],
      ["plan", "Plan & WBS"],
      ["organisation", "Organisation"],
      ["jobs", "Jobs"],
      ["procedures", "Procedures"],
      ["authoring", "Authoring"],
      ["improve", "Improve"],
      ["operations", "Operations"],
      ["toolkits", "Tools"],
      ["metering", "Metering"],
      ["architecture", "System architecture"],
      ["documents", "Documents"],
    ]) {
      await nav(id);
      check((await page.locator("h1").innerText()) === title, `${id} renders its title`);
      check(
        (await page.locator("#crumb").innerText()) === title,
        `${id} breadcrumb matches`,
      );
    }

    // 2b. The Documents destination reads the generated index, not a fixture.
    await nav("documents");
    check((await content()).includes("SPEC_V2"), "documents index lists SPEC_V2");
    check(
      (await page.locator('a[href="docs/SPEC_V2.html"]').count()) === 1,
      "documents index links SPEC_V2 as HTML",
    );
    // 2c. A generated document is a standalone page: chrome present, heading rendered,
    //     no external request (it must open from file://).
    await page.goto(
      pathToFileURL(path.resolve(__dirname, "../docs/SPEC_V2.html")).href,
    );
    await page.waitForSelector("h1");
    check(
      (await page.locator("h1").innerText()).includes(
        "organisational operating model",
      ),
      "SPEC_V2 HTML renders its heading",
    );
    check(
      (await page.locator(".doc-topbar").count()) === 1,
      "SPEC_V2 HTML carries the document chrome",
    );
    const localMarkdown = await page.$$eval("a[href]", (links) =>
      links.filter((a) => {
        const href = a.getAttribute("href") || "";
        return (
          !/^[a-z][a-z0-9+.-]*:/i.test(href) &&
          /\.md$/i.test(href.split("#")[0])
        );
      }).length,
    );
    check(localMarkdown === 0, "SPEC_V2 HTML links no local Markdown page");

    // 3. Portfolio: programme switch, computed WSJF, dependency cycle refusal.
    await nav("portfolio");
    check((await content()).includes("computed read model"), "portfolio is a read model");
    const programmeButtons = page.locator('[data-v2="programme"]');
    if ((await programmeButtons.count()) > 1) {
      const rowsBefore = await page.locator(".e-table tbody tr").count();
      const target = programmeButtons.nth(1);
      const label = await target.locator("strong").innerText();
      await target.click();
      check(
        (await content()).includes(label) &&
          (await page.locator(".e-table tbody tr").count()) !== rowsBefore,
        "programme switch re-scopes the in-flight set",
      );
    } else {
      // A live snapshot may carry one real programme; there is no second one to switch to.
      const label = await programmeButtons.first().locator("strong").innerText();
      check((await content()).includes(label), "the selected programme is rendered");
    }
    await tab("portfolio", "Backlog & WSJF");
    check((await content()).includes("3.2"), "WSJF is computed from typed inputs");
    await tab("portfolio", "Shared config & dependencies");
    await action("cycle");
    check(
      (await content()).includes("422 project_dependency_cycle"),
      "dependency cycle is refused",
    );

    // 4. Plan: the plan seed is the DESIGN fallback; change control keeps its contract.
    await nav("plan");
    await tab("plan", "Boards");
    check(
      (await content()).includes("Plan seed · DESIGN"),
      "the plan seed renders when no live boards exist",
    );
    check(
      (await content()).includes("architecture_task.READY"),
      "every plan unit names a real process",
    );
    check(
      (await page.locator('[data-v2-input="plan-seed-project"]').count()) === 1,
      "the plan seed is scoped by project",
    );
    await tab("plan", "Change control");
    await action("change-approve");
    check(
      (await content()).includes("422 change_rationale_required"),
      "change request without a rationale is refused",
    );
    await page
      .locator('[data-v2-input="rationale"]')
      .fill("Programme board approved the delegated identity scope.");
    await action("change-approve");
    check(
      (await content()).includes("Approved locally") &&
        (await content()).includes("existing runs keep B3 pins"),
      "change approval drafts a new baseline and leaves pins unchanged",
    );

    // 5. Organisation: dual control, decision ladder, refinement versions.
    await nav("organisation");
    await tab("organisation", "Approval policies");
    await action("self-approve");
    check(
      (await toastText()).includes("422 dual_control_required"),
      "self-approval is refused by dual control",
    );
    await tab("organisation", "Decision ladder");
    check(
      (await content()).includes("L0 · RULE") &&
        (await content()).includes("L1 · LAYA") &&
        (await content()).includes("L2 · GENERATION") &&
        (await content()).includes("L3 · HUMAN"),
      "the four-rung decision ladder is visible",
    );
    await tab("organisation", "Refinements & laya config");
    await action("refine");
    check((await content()).includes("v4"), "a refinement edit creates a new version");

    // 6. Jobs: immutable releases, bindings and the stale-binding refusal.
    await nav("jobs");
    check((await content()).includes("software.delivery.backend_engineer"), "job catalogue");
    await tab("jobs", "Releases");
    await action("publish-release");
    check(
      (await content()).includes("Publish gate opened"),
      "a draft release needs a publish gate",
    );
    await tab("jobs", "Bindings");
    await action("stale");
    check(
      (await content()).includes("409 job_binding_stale"),
      "a stale binding is refused before any model call",
    );
    await action("retire-binding");
    check(
      (await page.locator(".e-table tbody tr.selected").innerText()).includes("Retired"),
      "retiring a binding keeps history",
    );
    await tab("jobs", "Legacy import");
    check(
      (await content()).includes("not the live model"),
      "the historical 92-entry catalogue is import material",
    );

    // 7. Procedures: SOP drafts, PDDL refusal, PKO versioning, readiness.
    await nav("procedures");
    await tab("procedures", "SOP intake");
    await action("sop-publish");
    check(
      (await content()).includes("422 review_required"),
      "an unreviewed SOP-derived procedure is refused",
    );
    await action("sop-advance");
    await action("sop-publish");
    check(
      (await content()).includes("Published behind the review gate"),
      "a reviewed SOP publishes behind a human gate",
    );
    await tab("procedures", "PDDL steps");
    await action("pddl-check");
    check(
      (await toastText()).includes("422 pko_term_unknown"),
      "a PDDL step with an unknown PKO term is refused",
    );
    await tab("procedures", "PKO editor");
    await action("pko-edit");
    check((await content()).includes("pko@4"), "a PKO edit creates a new release");
    await tab("procedures", "Publish");
    await action("publish-procedure");
    check(
      (await toastText()).includes("409 procedure_not_ready"),
      "publication is blocked by an unpublished pack",
    );
    await action("toggle-pack");
    await action("publish-procedure");
    check(
      (await toastText()).includes("Publish gate opened"),
      "publication opens a human gate when readiness passes",
    );

    // 8. Authoring: chat drafts, dependency-blocked publication.
    await nav("authoring");
    await page
      .locator('[data-v2-input="brief"]')
      .fill("A small IT delivery department with PM, backend and verification jobs.");
    await action("intake-chat");
    check(
      (await toastText()).includes("draft behind a review gate"),
      "chat intake creates drafts only",
    );
    await tab("authoring", "Draft catalogue");
    await action("publish-asset");
    check(
      (await content()).includes("422 asset_dependency_unpublished"),
      "a draft with an unpublished dependency cannot publish",
    );
    await tab("authoring", "Dependency status");
    check(
      (await content()).includes("1 blocker"),
      "the dependency blocker is visible",
    );

    // 9. Improve: kata evidence, lessons, adoption.
    await nav("improve");
    await action("kata-open");
    check(
      (await toastText()).includes("422 kata_instrument_required"),
      "a kata without an instrument is not opened",
    );
    await tab("improve", "Six Gets");
    check(
      (await content()).includes("Go to See") &&
        (await content()).includes("Get to Stretch"),
      "the six Gets steer the improvement",
    );
    await tab("improve", "Coach");
    await action("coach-repeat");
    check(
      (await toastText()).includes("escalates to a human"),
      "a repeat interruption signature escalates to a human",
    );
    await tab("improve", "Lessons");
    await action("lesson-promote");
    check(
      (await content()).includes("422 lesson_evidence_insufficient"),
      "a lesson with one evidence ref cannot be promoted",
    );
    await action("lesson", '[data-id="LSN-14"]');
    await action("lesson-promote");
    check(
      (await content()).includes("Review gate opened"),
      "a lesson with two independent refs reaches review",
    );
    await tab("improve", "Adoption");
    await action("adopt");
    check((await content()).includes("Approved"), "adoption needs and records the human gate");

    // 10. Operations: dry-run, destructive gate, andon.
    await nav("operations");
    await tab("operations", "Runbooks");
    await action("dry-run");
    check(
      (await content()).includes("Dry-run passed"),
      "a destructive write needs a dry-run",
    );
    await action("destructive");
    check(
      (await toastText()).includes("dry-run + dual control"),
      "the destructive write requires the configured approvals",
    );
    await action("blocked");
    check(
      (await toastText()).includes("403 ops:write"),
      "executing without approval is refused",
    );
    await tab("operations", "Andon");
    await action("andon-toggle");
    check(
      (await content()).includes("Andon released"),
      "the andon cord is released by a person",
    );
    await action("andon-toggle");
    check((await content()).includes("ANDON PULLED"), "the andon cord is pulled");

    // 11. Tools: probe unbind, effect ceiling, call ledger, BYO, authoring.
    await nav("toolkits");
    await tab("toolkits", "Probes");
    await action("probe", '[data-key="coding"]');
    check(
      (await content()).includes("409 tool_not_ready") &&
        (await content()).includes("in-flight runs finish"),
      "a failed probe unbinds new runs",
    );
    await action("probe", '[data-key="coding"]');
    await tab("toolkits", "Bindings & grants");
    await action("ceiling", '[data-level="destructive"]');
    check(
      (await content()).includes("403 tool_effect_forbidden"),
      "a binding cannot widen its grant",
    );
    await action("ceiling", '[data-level="read"]');
    check(
      (await toastText()).includes("Ceiling narrowed"),
      "a binding narrows its effect ceiling",
    );
    await tab("toolkits", "Calls ledger");
    await action("call", '[data-id="TC-9002"]');
    check(
      (await content()).includes("coding.test_runs.create"),
      "the tool_call ledger is inspectable",
    );
    await tab("toolkits", "BYO import");
    await action("byo-import");
    check(
      (await toastText()).includes("Draft toolkit imported"),
      "a BYO toolkit enters as a draft with review",
    );
    await tab("toolkits", "Tool authoring");
    await action("tool-publish");
    check(
      (await toastText()).includes("cannot widen its own effect ceiling"),
      "a tool publication cannot widen its own ceiling",
    );

    // 12. Metering: one derivation, every level, drill-down.
    await nav("metering");
    check(
      (await content()).toLowerCase().includes("tenant spend"),
      "tenant roll-up is visible",
    );
    await action("meter", '[data-level="call"]');
    check(
      (await content()).includes("TC-9002") &&
        (await content()).toLowerCase().includes("selected level"),
      "metering drills to the tool call",
    );
    await tab("metering", "Attribution");
    check(
      (await content()).includes("unattributed") ||
        (await content()).includes("Unattributed"),
      "unattributed calls are reported separately",
    );
    await tab("metering", "Metrics & maturity");
    check(
      (await content()).includes("never refused") ||
        (await content()).includes("never-refused"),
      "a never-refused gate is flagged",
    );

    // 12b. The run inspector gains the V2 tools panel and the binding/release pin.
    await nav("execution");
    check(
      (await content()).includes("Run binding & tools") &&
        (await content()).includes("3f9c"),
      "run inspector exposes the tools panel and release pin",
    );
    await page.setViewportSize({ width: 390, height: 1060 });
    await nav("execution");
    check(!(await overflow()), "execution tools panel fits at 390px");
    await page.setViewportSize({ width: 1440, height: 1060 });

    // 13. The V1 surfaces are retained with a V2 banner.
    await nav("jobtypes");
    check(
      (await content()).includes("V1 SURFACE") &&
        (await content()).includes("not the live model"),
      "the historical job catalogue is banner-labelled",
    );
    await nav("tools");
    check(
      (await content()).includes("V1 SURFACE") &&
        (await content()).includes("n8n is reference only"),
      "the coding catalogue is labelled as a reference",
    );
    await nav("constraints");
    check(
      (await content()).includes("optional constraint-reasoning study"),
      "the TOC study is explicitly optional",
    );

    // 14. Responsive: every V2 destination at 1440, 1024 and 390.
    const destinations = [
      "portfolio",
      "plan",
      "organisation",
      "jobs",
      "procedures",
      "authoring",
      "improve",
      "operations",
      "toolkits",
      "metering",
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

    // 15. Screenshots for the six new workspaces plus the two extended ones.
    await page.setViewportSize({ width: 1440, height: 1060 });
    for (const id of [
      "portfolio",
      "plan",
      "organisation",
      "jobs",
      "procedures",
      "authoring",
      "improve",
      "operations",
      "toolkits",
      "metering",
    ]) {
      await nav(id);
      await page.evaluate(() => document.fonts.ready);
      await page.screenshot({ path: shot(`conveyor-v2-${id}.png`) });
    }
    await page.setViewportSize({ width: 390, height: 1060 });
    await nav("portfolio");
    await page.screenshot({ path: shot("conveyor-v2-portfolio-mobile.png") });
    await page.emulateMedia({ colorScheme: "dark" });
    await page.evaluate(() =>
      document.documentElement.setAttribute("data-theme", "dark"),
    );
    await nav("operations");
    await page.evaluate(() =>
      document.documentElement.setAttribute("data-theme", "dark"),
    );
    await page.screenshot({ path: shot("conveyor-v2-operations-dark.png") });

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
