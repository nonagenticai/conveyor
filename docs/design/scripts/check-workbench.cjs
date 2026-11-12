// Knowledge & memory workbench checks. Same optional local Playwright + Chrome setup as the
// other studio passes. Every record is a local fixture; no backend or network is used.
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
    const learning = async (name, filter = "") =>
      page.locator(`[data-l="${name}"]${filter}`).first().click();
    const enterprise = async (name, filter = "") =>
      page.locator(`[data-e="${name}"]${filter}`).first().click();
    const v2 = async (name, filter = "") =>
      page.locator(`[data-v2="${name}"]${filter}`).first().click();
    const ltab = async (key, value) =>
      learning("tab", `[data-key="${key}"][data-value="${value}"]`);
    const etab = async (value) =>
      enterprise("knowledge-tab", `[data-value="${value}"]`);
    const vtab = async (key, value) =>
      v2("tab", `[data-key="${key}"][data-value="${value}"]`);
    const content = async () => page.locator("#main").innerText();
    const toastText = async () => {
      await page.waitForTimeout(60);
      return page.locator("#toast").innerText();
    };
    const overflow = async () =>
      page.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1);
    const close = async () => {
      const open = page.locator("#dialog[open]");
      if (await open.count()) await page.keyboard.press("Escape");
    };
    const submit = async (id) => page.locator(`button[form="${id}"]`).click();

    // 1. The workbench destinations resolve and stay within the viewport.
    for (const width of [1440, 1024, 390]) {
      await page.setViewportSize({ width, height: 1060 });
      for (const dest of [
        "knowledge-design",
        "knowledge",
        "memory",
        "procedures",
        "execution",
        "evals",
      ]) {
        await nav(dest);
        check(
          (await page.locator("h1").count()) === 1,
          `${dest} has one h1 at ${width}px`,
        );
        check(!(await overflow()), `${dest} has no overflow at ${width}px`);
      }
    }
    await page.setViewportSize({ width: 1440, height: 1060 });

    // 2. The retained knowledge pages remain reachable and distinct.
    await nav("knowledge-design");
    check(
      (await content()).includes("Knowledge design"),
      "knowledge design page retained",
    );
    await nav("knowledge");
    check(
      (await content()).includes("Knowledge & memory"),
      "sources & retrieval page retained",
    );
    await nav("memory");
    check(
      (await content()).includes("Memory explorer"),
      "memory explorer page retained",
    );

    // 3. Sources & retrieval: transformation, passages, quality, retrieval test.
    await nav("knowledge");
    let text = await content();
    check(
      (await page.locator(".e-ingest-stages > div").count()) === 7,
      "the seven upstream ingestion stages remain",
    );
    check(
      text.includes("Transformation chain"),
      "the source transformation chain renders",
    );
    check(
      ["Imported", "Extracted", "Reviewed", "Published", "Eligible", "Retrieved", "Included"].every(
        (s) => text.includes(s),
      ),
      "the seven source states are distinct",
    );
    check(
      text.includes("Quality & ownership") && text.includes("A. Mensah"),
      "quality signals and the named verifier are visible",
    );
    check(
      text.includes("Used by") && text.includes("proc-implement@7"),
      "downstream consumers are named",
    );
    check(
      text.includes("p.42 · §3.2") &&
        text.includes("Original") &&
        text.includes("Extracted passage"),
      "the original and extracted passage are shown together",
    );
    await enterprise("passage-select", '[data-index="1"]');
    check(
      (await content()).includes("[table omitted by the parser]"),
      "a parse omission is visible in the passage inspector",
    );
    await enterprise("passage-toggle");
    check(
      (await content()).includes("Disable for retrieval"),
      "passage retrieval eligibility is toggleable",
    );
    await enterprise("passage-correct");
    await page
      .locator('#e-passage-form [name="rationale"]')
      .fill("The table must be recovered before publication.");
    await submit("e-passage-form");
    check(
      (await content()).includes("Correction is a versioned draft"),
      "a correction creates a reviewable draft",
    );
    await etab("Retrieval & citations");
    await page
      .locator('#e-retrieval-form [name="expected"]')
      .fill("a passage that does not exist in this fixture");
    await page.locator("#e-retrieval-form button").click();
    check(
      (await content()).includes("missed · candidate regression"),
      "a missed expected-evidence test is shown",
    );
    check(
      (await page.locator(".e-citation").count()) === 3,
      "the cited evidence fixture remains three passages",
    );
    await enterprise("save-case");
    await page
      .locator('#e-case-form [name="rationale"]')
      .fill("The expected evidence was absent from the returned passages.");
    await submit("e-case-form");
    check(
      (await content()).includes("Candidate case saved"),
      "a failed retrieval saves as a candidate case",
    );
    check(
      (await content()).includes("human review required"),
      "the candidate case requires a person's review",
    );

    // 4. The workbench is explicitly labelled as proposed.
    await nav("knowledge-design");
    await ltab("designTab", "Effective configuration");
    check(
      (await content()).includes("V2 · PROPOSED"),
      "effective configuration carries the V2 proposed ribbon",
    );

    // 5. Effective configuration: inheritance, conflict, views, dependents.
    text = await content();
    check(
      text.includes("department_conflict") && text.includes("verification"),
      "unresolved department conflict names the sibling department",
    );
    check(
      text.includes("Dependents of this draft") &&
        text.includes("proc-implement@7"),
      "dependents name the affected procedure pin",
    );
    check(text.includes("Blocked"), "unresolved conflict blocks the draft release");
    await learning("config-view", '[data-value="Current release"]');
    text = await content();
    check(
      text.includes("identity-v2 · job row J-12"),
      "current release shows the resolved published value",
    );
    check(
      !text.includes("department_conflict — unresolved"),
      "current release does not show the draft conflict",
    );
    await learning("config-view", '[data-value="Compare"]');
    text = await content();
    check(
      text.includes("Current release") && text.includes("Draft"),
      "compare shows the current and draft values",
    );
    await learning("config-view", '[data-value="Draft"]');
    await learning("config-resolve");
    await page
      .locator('#l-config-resolve-form [name="rationale"]')
      .fill("Verification owns the identity contract; the delivery exception is documented.");
    await submit("l-config-resolve-form");
    text = await content();
    check(
      text.includes("job-layer draft"),
      "resolving creates a job-layer draft row",
    );
    check(
      text.includes("Release draft resolves"),
      "resolving the conflict clears the block",
    );
    await learning("config-missing");
    check(
      (await content()).includes("409 release_not_ready"),
      "a missing required kind refuses the release",
    );
    await learning("config-missing");

    // 5. Review and release: queue, authority and the change-set lifecycle.
    await ltab("designTab", "Review & release");
    text = await content();
    check(text.includes("Review and release"), "review queue renders");
    check(text.includes("Source owner"), "queue names the required reviewer");
    check(text.includes("dual control"), "binding revision needs dual control");
    check(text.includes("forbidden"), "model/agent approval is forbidden");
    await learning("review-agent");
    check(
      (await toastText()).includes("403 human_approval_required"),
      "an agent approval attempt is refused",
    );
    await learning("review-select", '[data-id="RV-02"]');
    await learning("review-advance");
    check(
      (await page.locator('tr:has(small:text-is("RV-02"))').innerText()).includes(
        "In review",
      ),
      "a draft change set enters review",
    );
    await learning("review-advance");
    await page
      .locator('#l-review-form [name="rationale"]')
      .fill("Evidence reviewed; a person approves this change set.");
    await submit("l-review-form");
    check(
      (await page.locator('tr:has(small:text-is("RV-02"))').innerText()).includes(
        "Approved",
      ),
      "a person's rationale approves the change set",
    );
    await learning("review-advance");
    check(
      (await page.locator('tr:has(small:text-is("RV-02"))').innerText()).includes(
        "Published",
      ),
      "an approved change set publishes",
    );
    await learning("review-select", '[data-id="RV-04"]');
    await learning("review-rollback");
    await page
      .locator('#l-rollback-form [name="rationale"]')
      .fill("Regression found; roll back and keep the audit.");
    await submit("l-rollback-form");
    check(
      (await page.locator('tr:has(small:text-is("RV-04"))').innerText()).includes(
        "Superseded",
      ),
      "rollback marks the published release superseded",
    );
    check(
      (await content()).includes("rollback draft"),
      "rollback creates a new draft version",
    );

    // 6. The workflow strip links to the proving surfaces.
    await ltab("designTab", "Effective configuration");
    await learning("workflow-go", '[data-step="5"]');
    check(
      (await content()).includes("Compare"),
      "workflow step 5 opens the comparison",
    );
    await learning("workflow-go", '[data-step="7"]');
    check(
      (await content()).includes("Review and release"),
      "workflow step 7 opens the review queue",
    );

    // 7. Memory record lifecycle: taxonomy, views, actions and dimensions.
    await nav("memory");
    text = await content();
    check(
      text.includes("What is this?") &&
        text.includes("Where did it originate?") &&
        text.includes("Who can use it?"),
      "the six-question record inspector renders",
    );
    await learning("memory-view", '[data-value="History"]');
    check(
      (await content()).includes("No corrections yet"),
      "history shows the retained original before a correction",
    );
    await learning("memory-view", '[data-value="Derivation"]');
    text = await content();
    check(
      text.includes("Cold digest") &&
        text.includes("Included hot rows") &&
        text.includes("Conflicts"),
      "derivation shows the digest, inclusions and conflicts",
    );
    await learning("memory-digest");
    await page
      .locator('#l-digest-form [name="rationale"]')
      .fill("Confirm the dry-run diff; the hot rows stay authoritative.");
    await submit("l-digest-form");
    check(
      (await content()).includes("Digest rebuilt v5"),
      "digest rebuild requires a confirmed dry run",
    );
    await learning("memory-view", '[data-value="History"]');
    await learning("memory-successor");
    await page
      .locator('#l-successor-form [name="text"]')
      .fill("The delegation edge is verified by the observer record.");
    await page
      .locator('#l-successor-form [name="rationale"]')
      .fill("Correct the claim with independent evidence.");
    await submit("l-successor-form");
    text = await content();
    check(
      text.includes("m-card-2") && text.includes("Original retained"),
      "correction adds a successor and retains the original",
    );
    await learning("memory-view", '[data-value="Current"]');
    await learning("memory-exclude");
    await page
      .locator('#l-exclude-form [name="reason"]')
      .fill("Claim is superseded; keep it out of agent recall.");
    await submit("l-exclude-form");
    check(
      (await content()).includes("excluded ·"),
      "exclusion changes agent recall and records a reason",
    );
    await learning("memory-dim", '[data-value="Organisation"]');
    text = await content();
    check(
      text.includes("Industry") && text.includes("Department"),
      "the organisation dimension exposes industry and department levels",
    );
    await learning("memory-level", '[data-value="Industry"]');
    check(
      (await content()).includes("Industry memory"),
      "the industry level has a distinct record",
    );
    await learning("memory-dim", '[data-value="Lifecycle"]');
    await learning("memory-level", '[data-value="Lesson"]');
    check(
      (await content()).includes("Lesson memory"),
      "the lesson level has a distinct record",
    );
    await ltab("memoryTab", "Promotion trail");
    text = await content();
    check(
      text.includes("second independent") && text.includes("≥2 independent"),
      "promotion shows the independent-evidence requirement",
    );
    await ltab("memoryTab", "Records");
    await learning("memory-policy-compare");
    text = await content();
    check(
      text.includes("Memory policy comparison") &&
        text.includes("Candidate policy"),
      "memory policy comparison shows the three arms",
    );

    // 8. PKO term workspace: detail, formal view, usage, graph and refusals.
    await nav("procedures");
    await vtab("procedures", "PKO editor");
    text = await content();
    check(
      text.includes("Term · verify_release") &&
        text.includes("Permitted relationships"),
      "the PKO term inspector renders definition and permitted relations",
    );
    check(
      text.includes("Procedure steps using this term") &&
        text.includes("proc-implement v7 · step 04"),
      "term usage links to the procedure steps",
    );
    check(
      text.includes("Relationships · 2 hops"),
      "the local relationship graph renders",
    );
    await v2("pko-term", '[data-id="PKO-T02"]');
    check(
      (await content()).includes("Term · approve_baseline"),
      "selecting a term refreshes its detail",
    );
    await v2("pko-formal");
    check(
      (await content()).includes("entity PKO-T02") &&
        (await content()).includes("constraints:"),
      "the formal representation is behind progressive disclosure",
    );
    await v2("pko-kind", '[data-id="task"]');
    check(
      (await page.locator('[data-v2="pko-term"]').count()) === 2,
      "the term table is filterable by kind",
    );
    await page.locator('[data-v2-input="pko-search"]').fill("approve");
    await page.locator('[data-v2-input="pko-search"]').press("Enter");
    check(
      (await page.locator('[data-v2="pko-term"]').count()) === 1,
      "the PKO term table is searchable",
    );
    await page.locator('[data-v2-input="pko-search"]').fill("");
    await page.locator('[data-v2-input="pko-search"]').press("Enter");
    await v2("pko-validate");
    check(
      (await toastText()).includes("422 pko_relation_invalid"),
      "an invalid PKO relation is refused",
    );
    await v2("pko-add-relation");
    check(
      (await toastText()).includes("422 pko_relation_invalid"),
      "adding an invalid relation is refused",
    );
    await v2("pko-publish-check");
    check(
      (await toastText()).includes("409 pko_not_ready"),
      "publish readiness refuses an undefined term",
    );
    await v2("pko-edit");
    check(
      (await content()).includes("pko@4"),
      "a PKO edit creates a new immutable release draft",
    );

    // 9. Explain context: manifest, states, reasons, hash and reconstruction.
    await nav("execution");
    text = await content();
    check(
      text.includes("Explain context · RUN-151"),
      "the explain-context panel renders",
    );
    check(
      text.includes("excluded") && text.includes("truncated"),
      "excluded and truncated items are visible",
    );
    check(
      text.includes("parse omission") && text.includes("hot rows win"),
      "selection and exclusion reasons are named",
    );
    check(
      text.includes("prompt_sha256") &&
        text.includes("never renders the compiled prompt"),
      "the manifest exposes the hash and never the compiled prompt",
    );
    check(
      text.includes("Reconstruction available"),
      "the reconstruction status is explicit",
    );
    await v2("explain-retired");
    check(
      (await content()).includes("ING-028"),
      "a retired source makes exact reconstruction impossible",
    );
    await v2("explain-missing");
    check(
      (await content()).includes("409 capability_not_ready"),
      "a missing required kind refuses the run before a model call",
    );

    // 10. Object evaluation: subjects, memory arms, statuses and replay cutoff.
    await nav("evals");
    await ltab("evalTab", "Object evaluation");
    text = await content();
    check(
      [
        "Knowledge extraction",
        "Retrieval",
        "Procedural knowledge (PKO)",
        "Memory",
        "Scope policies",
        "Job outcomes",
      ].every((s) => text.includes(s)),
      "the six evaluation subjects are present",
    );
    await learning("eval-subject", '[data-id="memory"]');
    text = await content();
    check(
      text.includes("Memory policy comparison") &&
        text.includes("Memory disabled"),
      "memory comparison renders the disabled arm",
    );
    check(
      text.includes("Candidate policy") &&
        text.includes("±6.5 pp noise floor") &&
        text.includes("$0.42"),
      "the comparison shows uncertainty and cost",
    );
    check(
      text.includes("Human review") &&
        text.includes("Model assessment") &&
        text.includes("Measured"),
      "human, model and measured statuses are separate",
    );
    await page
      .locator('#l-replay-form [name="memory"]')
      .selectOption("current");
    await page
      .locator('#l-replay-form [name="rationale"]')
      .fill("Replay the retrieval candidate.");
    await page.locator("#l-replay-form button").click();
    check(
      (await content()).includes("Replay requires an isolated memory state"),
      "a live-memory replay is refused",
    );
    await page
      .locator('#l-replay-form [name="memory"]')
      .selectOption("isolated");
    await page
      .locator('#l-replay-form [name="rationale"]')
      .fill("Compare the candidate on an isolated historical state.");
    await page.locator("#l-replay-form button").click();
    check(
      (await content()).includes("Replay scheduled"),
      "an isolated replay is scheduled",
    );
    await nav("knowledge");
    await enterprise("source-eval");
    check(
      (await page.locator("h1").innerText()) === "Evaluation studio",
      "a source links to its object evaluation",
    );
    check(
      (await content()).includes("Object evaluation") &&
        (await content()).includes("Retrieval"),
      "the source entry point selects the retrieval subject",
    );
    await nav("knowledge");
    await etab("Packs & bindings");
    await enterprise("pack-eval");
    check(
      (await content()).includes("Knowledge extraction · extraction-suite v2"),
      "a pack links to its extraction evaluation",
    );
    await nav("memory");
    await learning("memory-policy-compare");
    await learning("eval-object", '[data-subject="scope"]');
    check(
      (await content()).includes("Scope policies · isolation-suite v1"),
      "a scope policy links to its evaluation",
    );
    await nav("jobs");
    await v2("job-eval");
    check(
      (await content()).includes("Job outcomes · architecture-v3"),
      "a job links to its outcome evaluation",
    );

    // 10. The new PKO and explain-context surfaces fit at 390px.
    await page.setViewportSize({ width: 390, height: 1060 });
    await nav("procedures");
    await vtab("procedures", "PKO editor");
    check(!(await overflow()), "the PKO workspace fits at 390px");
    await nav("execution");
    check(!(await overflow()), "the explain-context panel fits at 390px");
    await page.setViewportSize({ width: 1440, height: 1060 });

    // 11. Screenshots for the workbench surfaces.
    await page.setViewportSize({ width: 1440, height: 1060 });
    await page.emulateMedia({ colorScheme: "light" });
    await nav("knowledge-design");
    await ltab("designTab", "Effective configuration");
    await page.evaluate(() => document.fonts.ready);
    await page.screenshot({
      path: shot("conveyor-workbench-knowledge-design.png"),
    });
    await nav("knowledge");
    await page.screenshot({ path: shot("conveyor-workbench-sources.png") });
    await nav("memory");
    await page.screenshot({ path: shot("conveyor-workbench-memory.png") });
    await nav("procedures");
    await vtab("procedures", "PKO editor");
    await page.screenshot({ path: shot("conveyor-workbench-pko.png") });
    await nav("execution");
    await page.screenshot({ path: shot("conveyor-workbench-execution.png") });
    await nav("evals");
    await ltab("evalTab", "Object evaluation");
    await page.screenshot({ path: shot("conveyor-workbench-evals.png") });
    await page.setViewportSize({ width: 390, height: 1060 });
    await nav("knowledge-design");
    await ltab("designTab", "Effective configuration");
    await page.screenshot({ path: shot("conveyor-workbench-mobile.png") });
    await page.emulateMedia({ colorScheme: "dark" });
    await nav("execution");
    await page.evaluate(() =>
      document.documentElement.setAttribute("data-theme", "dark"),
    );
    await page.screenshot({ path: shot("conveyor-workbench-dark.png") });

    // 12. No runtime errors, failed assets or external requests.
    check(errors.length === 0, "no runtime errors");
    check(failed.length === 0, "no failed local assets");
    check(external.length === 0, "no external page requests");

    await page.screenshot({ path: shot("conveyor-workbench-final.png") });
    console.log(`Completed ${count} knowledge & memory workbench checks.`);
  } finally {
    await browser.close();
  }
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
