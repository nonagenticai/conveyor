// Behavioural and responsive checks for the knowledge / memory / measurement study.
// Uses the same optional local Playwright + Chrome dependencies as check-studio.cjs.
const { chromium } = require("playwright-core");
const assert = require("node:assert/strict");
const path = require("node:path");
const os = require("node:os");
const { pathToFileURL } = require("node:url");
const base = pathToFileURL(path.resolve(__dirname, "../studio.html")).href;

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
      page.locator(`[data-l="${name}"]${filter}`).first().click();
    const tab = async (key, value) =>
      action("tab", `[data-key="${key}"][data-value="${value}"]`);
    const content = async () => page.locator("#main").innerText();
    const dialog = async () => page.locator("#dialog").innerText();
    const close = async () => {
      if (await page.locator("#dialog[open]").count())
        await page
          .locator('#dialog [data-action="close-dialog"]')
          .first()
          .click();
    };
    const submit = async (id) => page.locator(`button[form="${id}"]`).click();
    const fill = async (id, name, value) =>
      page.locator(`#${id} [name="${name}"]`).fill(value);

    await nav("overview");
    check(
      (await page.locator(".l-operating-strip button").count()) === 3,
      "programme entry exposes design, memory and improvement",
    );
    await nav("knowledge-design");
    check(
      (await content()).includes("Inherited") &&
        (await content()).includes("Narrowed"),
      "effective context distinguishes inheritance and selection",
    );
    for (const scope of ["process", "agent", "run"]) {
      await action("design-scope", `[data-scope="${scope}"]`);
      check(
        (await page
          .locator(`.l-tree [data-scope="${scope}"].selected`)
          .count()) === 1,
        "select context scope " + scope,
      );
    }
    await action("design-scope", '[data-scope="capability"]');
    await action("edit-selection");
    await page
      .locator('#l-selection-form [name="selection"]')
      .selectOption("none");
    await submit("l-selection-form");
    await action("compile");
    check(
      (await dialog()).includes("409 capability_not_ready"),
      "empty required knowledge refuses compilation",
    );
    await close();
    await action("edit-selection");
    await page
      .locator('#l-selection-form [name="selection"]')
      .selectOption("both");
    await submit("l-selection-form");
    await action("compile");
    check(
      (await dialog()).includes("8,160 / 12,000"),
      "compiler uses character budgets and source manifest",
    );
    await action("budget-refuse");
    check(
      (await dialog()).includes("SectionOverBudget"),
      "budget overrun is a visible refusal",
    );
    await close();
    await action("design-outage");
    await action("compile");
    check(
      (await dialog()).includes("409 capability_not_ready"),
      "missing required source refuses before model call",
    );
    await close();
    await action("design-outage");
    await page.locator('[data-l-change="capability"]').selectOption("delivery");
    check(
      (await content()).includes("Repository engineering standards"),
      "capability switch changes actual knowledge sources",
    );
    await action("design-scope", '[data-scope="process"]');
    await action("edit-selection");
    await page
      .locator('#l-selection-form [name="selection"]')
      .selectOption("standards");
    await submit("l-selection-form");
    check(
      (await content()).includes("2 selected / 3 available"),
      "process narrows its own knowledge selection",
    );
    await action("design-scope", '[data-scope="capability"]');
    check(
      (await content()).includes("3 selected / 3 available"),
      "process edit preserves capability source contract",
    );
    await action("design-scope", '[data-scope="agent"]');
    check(
      (await content()).includes("2 selected / 3 available"),
      "agent inherits narrowed process selection",
    );
    await tab("designTab", "Memory policy");
    await action("edit-memory-policy");
    await page
      .locator('#l-memory-policy-form [name="policy"]')
      .selectOption("hot_only");
    await submit("l-memory-policy-form");
    check(
      (await content()).includes("Same-card hot rows only"),
      "agent memory policy narrows recall to hot rows",
    );
    await action("design-scope", '[data-scope="capability"]');
    check(
      (await content()).includes("Same-card hot rows + cold digest"),
      "agent memory policy preserves parent contract",
    );

    await nav("capabilities");
    await action("design-entry");
    check(
      (await page.locator("h1").innerText()) === "Knowledge design",
      "definition links into scoped knowledge design",
    );
    await nav("memory");
    for (const scope of [
      "Tenant",
      "Programme",
      "Project",
      "Capability",
      "Process",
      "Agent",
      "Work package",
      "Run / attempt",
    ]) {
      await action("memory-level", `[data-value="${scope}"]`);
      check(
        (await content()).includes(scope + " memory"),
        "memory scope " + scope + " has a distinct record",
      );
    }
    await action("memory-level", '[data-value="Work package"]');
    check(
      (await content()).includes(
        "Producer output cannot establish verification",
      ),
      "claim status is not confused with verification",
    );
    await tab("memoryTab", "Recall & isolation");
    await page.locator('#l-recall-form [name="card"]').selectOption("WP-2061");
    await page.locator("#l-recall-form button").click();
    check(
      (await content()).includes("No fixture memory for WP-2061"),
      "recall returns no other-card memory on empty scope",
    );
    check(
      !(await page.locator(".l-note").allTextContents()).some(
        (t) => t.startsWith("No fixture") && t.includes("Hot: delegation"),
      ),
      "empty scope has no leaked card digest",
    );
    await action("promote-memory");
    await submit("l-promote-form");
    check(
      (await content()).includes("Candidate waiting for owner review"),
      "lesson proposal does not publish knowledge",
    );
    await action("promote-memory");
    await page
      .locator('#l-promote-form [name="decision"]')
      .selectOption("approve");
    await submit("l-promote-form");
    check(
      (await page.locator("#form-error:visible").count()) === 1,
      "lesson approval needs rationale and independent evidence",
    );
    await close();

    await nav("evals");
    await action("eval-run");
    check(
      (await dialog()).includes("grading"),
      "eval run refused without grading sign-off",
    );
    await close();
    await action("eval-sign", '[data-gate="grading"]');
    await submit("l-eval-sign-form");
    check(
      (await page.locator("#form-error:visible").count()) === 1,
      "sign-off requires recorded person rationale",
    );
    await fill(
      "l-eval-sign-form",
      "rationale",
      "Reviewed pilot labels, controls and rubric.",
    );
    await submit("l-eval-sign-form");
    await action("eval-run");
    check(
      (await content()).includes("sample pass complete"),
      "signed instrument permits local run replay",
    );
    await tab("evalTab", "Cases & splits");
    await page.locator('[data-l-change="actor"]').selectOption("analyzer");
    check(
      (await page.locator('[data-l="case-open"][data-id="AC-161"]').count()) ===
        0,
      "analyzer view excludes held-out trace link",
    );
    await tab("evalTab", "Sign-offs");
    await action("eval-sign");
    check(
      (await dialog()).includes("Reviewer access required"),
      "analyzer cannot approve instrument",
    );
    await close();
    await page.locator('[data-l-change="actor"]').selectOption("reviewer");
    await tab("evalTab", "Instrument");
    await action("edit-eval");
    await submit("l-edit-eval-form");
    await action("eval-run");
    check(
      (await dialog()).includes("inputs, grading, cost, harness"),
      "instrument edit invalidates every artifact sign-off",
    );
    await close();
    await action("eval-select", '[data-id="delivery"]');
    check(
      (await content()).includes("no results are invented"),
      "switching instrument does not reuse architecture results",
    );
    await action("eval-select", '[data-id="architecture"]');
    check(
      (await content()).includes("Instrument @4"),
      "returning to an instrument preserves its revised version",
    );
    await action("eval-run");
    check(
      (await dialog()).includes("inputs, grading, cost, harness"),
      "another instrument cannot restore invalidated approvals",
    );
    await close();
    await action("new-eval");
    await fill("l-new-eval-form", "name", "Governed tenant outcome");
    await fill(
      "l-new-eval-form",
      "purpose",
      "Assess a tenant-specific capability outcome.",
    );
    await submit("l-new-eval-form");
    check(
      (await content()).includes("Governed tenant outcome"),
      "tenant can create a separate instrument draft",
    );
    await action("add-case");
    await fill("l-case-form", "name", "Missing acceptance evidence");
    await fill(
      "l-case-form",
      "input",
      "Review the final artefact against its requirement baseline.",
    );
    await submit("l-case-form");
    check(
      (await content()).includes("1 illustrative cases"),
      "case authoring belongs to the new instrument draft",
    );

    await nav("hillclimb");
    await action("propose-round");
    check(
      (await dialog()).includes("Unsigned plan"),
      "new climb round refused without signed plan",
    );
    await close();
    await action("climb-plan");
    await fill("l-plan-form", "delta", "2");
    await submit("l-plan-form");
    check(
      (await page.locator("#form-error").innerText()).includes("6.5"),
      "plan refuses decision band below noise floor",
    );
    await close();
    await action("climb-sign");
    await fill(
      "l-climb-sign-form",
      "rationale",
      "Accept fixed split, $300 ceiling and stopping rules.",
    );
    await submit("l-climb-sign-form");
    await action("propose-round");
    check(
      (await dialog()).includes("Stall triage required"),
      "two flat rounds force triage before more content",
    );
    await close();
    await action("triage");
    await submit("l-triage-form");
    check(
      (await content()).includes("Triage recorded"),
      "triage round records failure buckets",
    );
    await action("propose-round");
    await fill("l-round-form", "gain", "2");
    await submit("l-round-form");
    check(
      (await page.locator("#form-error").innerText()).includes("below"),
      "hypothesis gain below signed band refused",
    );
    await fill("l-round-form", "gain", "9");
    await submit("l-round-form");
    check(
      (await dialog()).includes("Review proposed round"),
      "one round waits for configured person review",
    );
    await fill(
      "l-round-sign-form",
      "rationale",
      "Approve this bounded replay and inspect the held-out result.",
    );
    await submit("l-round-sign-form");
    check(
      (await content()).includes("Train ↑ / test flat ⇒ revert"),
      "training-only gain is reverted",
    );
    await tab("climbTab", "Paired comparison");
    check(
      (await content()).includes("0.0075"),
      "paired comparison computes exact two-sided McNemar",
    );
    await action("compare-refuse");
    check(
      (await dialog()).includes("409 not_comparable"),
      "different instrument versions refuse comparison",
    );
    await close();
    await page.locator('[data-l-change="actor"]').selectOption("analyzer");
    check(
      !(await content()).includes("PAIR 1"),
      "held-out transcript pairs absent from analyzer DOM",
    );
    await page.locator('[data-l-change="actor"]').selectOption("reviewer");
    await action("climb-stop");
    await fill(
      "l-stop-form",
      "rationale",
      "Stop after plateau; v1 is the measured winner.",
    );
    await submit("l-stop-form");
    await action("adopt-winner");
    await fill(
      "l-adopt-form",
      "rationale",
      "Prepare draft for separate publication review.",
    );
    await submit("l-adopt-form");
    check(
      (await content()).includes("published @2 remains active"),
      "adoption creates reviewed draft and preserves published version",
    );
    await page.locator('[data-l-change="climb"]').selectOption("cost");
    check(
      (await content()).includes("Plan sign-off required"),
      "cost campaign has a separate plan signature from stopped quality climb",
    );
    await action("cost-refuse");
    check(
      (await dialog()).includes("76%"),
      "cheaper model refused when registered quality floor fails",
    );
    await close();

    await nav("benchmarks");
    await action("bench-tab", '[data-value="Paired comparison"]');
    check(
      (await content()).includes("0.0129"),
      "benchmark sample paired comparison matches fixture counts",
    );
    await page.locator('[name="bench-b"]').selectOption("incompatible");
    await action("bench-compare");
    check(
      (await content()).includes("409 not_comparable"),
      "different oracle or prompt refuses benchmark comparison",
    );
    await tab("benchTab", "Runs & controls");
    await action("bench-control-refuse");
    check(
      (await dialog()).includes("Control cannot be subject baseline"),
      "gold control cannot be promoted into subject baseline",
    );
    await close();
    await nav("usage");
    await action("audit-start");
    await action("audit-review");
    await fill(
      "l-audit-form",
      "rationale",
      "Accept finding as an experiment candidate.",
    );
    await submit("l-audit-form");
    check(
      (await content()).includes("Accepted"),
      "prompt finding transition records person review",
    );
    await tab("auditTab", "Audit policy");
    await action("audit-publish-refuse");
    check(
      (await dialog()).includes("422 procedure_audit_findings"),
      "high-confidence prompt audit finding blocks publication",
    );
    await close();

    const destinations = [
      "overview",
      "knowledge-design",
      "knowledge",
      "memory",
      "improvement",
      "hillclimb",
      "evals",
      "benchmarks",
      "usage",
    ];
    for (const width of [1440, 1024, 390]) {
      await page.setViewportSize({ width, height: 1060 });
      for (const dest of destinations) {
        await nav(dest);
        const overflow = await page.evaluate(
          () => document.documentElement.scrollWidth > innerWidth + 1,
        );
        check(!overflow, `${dest} has no page overflow at ${width}px`);
      }
      for (const [dest, key, values] of [
        [
          "knowledge-design",
          "designTab",
          ["Memory policy", "Readiness & impact"],
        ],
        ["memory", "memoryTab", ["Recall & isolation", "Promotion trail"]],
        [
          "evals",
          "evalTab",
          [
            "Cases & splits",
            "Run & results",
            "Report",
            "Audit",
            "Sign-offs",
            "Interop",
          ],
        ],
        [
          "hillclimb",
          "climbTab",
          ["Plan & boundaries", "Paired comparison", "Final report"],
        ],
        ["benchmarks", "benchTab", ["Paired comparison", "Kata trend"]],
        ["usage", "auditTab", ["Prompt audit", "Audit policy"]],
      ]) {
        await nav(dest);
        for (const value of values) {
          await tab(key, value);
          check(
            !(await page.evaluate(
              () => document.documentElement.scrollWidth > innerWidth + 1,
            )),
            `${dest} / ${value} fits at ${width}px`,
          );
        }
      }
    }
    await page.setViewportSize({ width: 1440, height: 1060 });
    for (const dest of [
      "knowledge-design",
      "memory",
      "improvement",
      "hillclimb",
      "evals",
      "benchmarks",
    ]) {
      await nav(dest);
      await page.evaluate(() => document.fonts.ready);
      await page.screenshot({
        path: path.join(
          process.env.CONVEYOR_QA_OUTPUT || os.tmpdir(),
          `conveyor-${dest}-v3.png`,
        ),
        fullPage: true,
      });
    }
    await page.setViewportSize({ width: 390, height: 844 });
    await nav("knowledge-design");
    await page.screenshot({
      path: path.join(
        process.env.CONVEYOR_QA_OUTPUT || os.tmpdir(),
        "conveyor-knowledge-mobile-v3.png",
      ),
      fullPage: true,
    });
    await page.setViewportSize({ width: 1440, height: 1060 });
    await nav("hillclimb");
    await page.locator('[data-action="theme"]').click();
    await page.screenshot({
      path: path.join(
        process.env.CONVEYOR_QA_OUTPUT || os.tmpdir(),
        "conveyor-hillclimb-dark-v3.png",
      ),
      fullPage: true,
    });
    check(
      errors.length === 0,
      "no browser runtime errors: " + errors.join("; "),
    );
    check(failed.length === 0, "all local assets loaded");
    check(external.length === 0, "no external requests");
    console.log(`Completed ${count} knowledge, memory and improvement checks.`);
  } finally {
    await browser.close();
  }
})().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
