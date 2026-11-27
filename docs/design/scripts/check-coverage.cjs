#!/usr/bin/env node
/*
 * check-coverage.cjs — the measured programme's gate.
 *
 * `docs/design/data/coverage.json` is the design pack's claim about what is real. This script
 * is what makes the claim checkable on every PR:
 *
 *   - every project and screen status is in the vocabulary;
 *   - every evidence path resolves (a status without evidence is the failure mode);
 *   - every partial/absent/proposed row names its gap;
 *   - every invoked action resolves in the compiled registry dump;
 *   - every v2.js page has a coverage row and no row names a page that does not exist;
 *   - every V2 slice resolves to its SPEC_V2 §12 row;
 *   - every V2 slice is declared in `scripts/v2-slice-artefacts.json` (artefacts, or an
 *     explicit evidence+gap row), its measured counts agree with its status, and no artefact
 *     is credited to two V2 slices (the generator refuses a V1↔V2 duplicate);
 *   - every project's units exist in the issue registry;
 *   - the seed and the coverage agree on the project set, and no dependency dangles;
 *   - no non-built project is without an open unit.
 *
 * `--self-test` proves the gate refuses: a bad action, a missing page row, an uncovered
 * project, a missing evidence path and a V2 slice that claims `built` with a missing
 * artefact each exit non-zero.
 *
 * Run: node docs/design/scripts/check-coverage.cjs [--self-test]
 */
"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..", "..", "..");
const read = (file) => fs.readFileSync(path.join(root, file), "utf8");

const PROJECT_STATUSES = new Set([
  "built",
  "partial",
  "planned",
  "absent",
  "proposed",
  "archived",
  "standing",
]);
const SCREEN_STATUSES = new Set(["live", "mixed", "proposed", "legacy"]);
const UNBUILT = new Set(["partial", "planned", "absent", "proposed"]);

function pageKeys(text) {
  const start = text.indexOf("const pages = {");
  assert(start >= 0, "v2.js has no `const pages = {` map");
  const brace = text.indexOf("{", start);
  let depth = 0;
  let end = -1;
  for (let index = brace; index < text.length; index += 1) {
    if (text[index] === "{") depth += 1;
    if (text[index] === "}") {
      depth -= 1;
      if (depth === 0) {
        end = index;
        break;
      }
    }
  }
  assert(end > brace, "the v2.js page map is not closed");
  const keys = [];
  for (const line of text.slice(brace, end).split("\n")) {
    const match = /^    ([a-z][a-z0-9_]*)\s*[:,]/.exec(line);
    if (match) keys.push(match[1]);
  }
  return keys;
}

function validate(document, context) {
  const actions = context.actions;
  const issues = context.issues;
  const seed = context.seed;
  const spec = context.spec;
  const v2 = context.v2;
  const exists = (file) => fs.existsSync(path.join(context.root, file));

  assert.equal(document.version, 1, "coverage.json version");
  assert(Array.isArray(document.projects) && document.projects.length > 0, "projects");
  assert(Array.isArray(document.screens) && document.screens.length > 0, "screens");

  const projectKeys = new Set();
  for (const project of document.projects) {
    assert(!projectKeys.has(project.key), `duplicate project ${project.key}`);
    projectKeys.add(project.key);
    assert(
      PROJECT_STATUSES.has(project.status),
      `${project.key} has unknown status ${project.status}`,
    );
    assert(Array.isArray(project.evidence) && project.evidence.length > 0, `${project.key} evidence`);
    for (const evidence of project.evidence) {
      assert(exists(evidence), `${project.key} evidence does not resolve: ${evidence}`);
    }
    if (UNBUILT.has(project.status)) {
      assert(project.gap, `${project.key} is ${project.status} with no gap`);
    }
    if (project.plan) {
      const plan = project.plan.split("#")[0];
      assert(exists(plan), `${project.key} plan does not resolve: ${plan}`);
    }
    if (project.track === "v2") {
      assert(spec.includes(`**${project.key}**`), `${project.key} is not in SPEC_V2 §12`);
    }
    for (const number of project.units.numbers) {
      assert(issues.has(number), `${project.key} unit #${number} is not in issues.json`);
    }
    assert.equal(project.units.unknown, 0, `${project.key} has an unknown unit state`);
  }

  // The V2 declaration: every slice is declared, every declared artefact is credited once, and
  // the measured counts are honest. This is what stops `track === "v2"` becoming a hardcoded
  // `proposed` again.
  const declared = context.v2art;
  const credited = new Map();
  const v2Projects = document.projects.filter((project) => project.track === "v2");
  for (const project of v2Projects) {
    const declaration = declared[project.key];
    assert(declaration, `${project.key} has no declaration in scripts/v2-slice-artefacts.json`);
    const artefacts = [
      ...(declaration.tables || []).map((name) => `table:${name}`),
      ...(declaration.actions || []).map((name) => `action:${name}`),
      ...(declaration.modules || []).map((name) => `module:${name}`),
    ];
    for (const artefact of artefacts) {
      assert(
        !credited.has(artefact),
        `${artefact} is credited to both ${credited.get(artefact)} and ${project.key}`,
      );
      credited.set(artefact, project.key);
    }
    if (artefacts.length === 0) {
      assert(
        Array.isArray(declaration.evidence) && declaration.evidence.length > 0,
        `${project.key} declares no artefacts and no evidence row`,
      );
      const delivered = Array.isArray(declaration.delivered) ? declaration.delivered : [];
      if (UNBUILT.has(project.status)) {
        assert(declaration.gap, `${project.key} declares no artefacts and no gap`);
        assert(
          project.status === "proposed" || project.status === "planned",
          `${project.key} is curated but its measured status is ${project.status}`,
        );
      } else {
        // #882: a behavioural slice may be built, but only from a delivered proof — the routes
        // and specs that carry the behaviour. Every path must resolve, so a bare status flip
        // does not pass.
        assert(
          delivered.length > 0,
          `${project.key} is curated ${project.status} without a delivered proof`,
        );
        for (const path of delivered) {
          assert(
            exists(path),
            `${project.key} delivered path does not resolve: ${path}`,
          );
        }
      }
      continue;
    }
    const measured = [
      project.measured.tables,
      project.measured.actions,
      project.measured.modules,
    ];
    const counted = measured.reduce((sum, [, total]) => sum + total, 0);
    const present = measured.reduce((sum, [found]) => sum + found, 0);
    assert.equal(
      counted,
      artefacts.length,
      `${project.key} measures ${counted} of ${artefacts.length} declared artefacts`,
    );
    if (project.status === "built") {
      assert.equal(
        present,
        counted,
        `${project.key} is built with ${present}/${counted} artefacts present`,
      );
    } else if (project.status === "partial") {
      assert(
        present > 0 && present < counted,
        `${project.key} is partial with ${present}/${counted} artefacts present`,
      );
    } else {
      assert.equal(
        present,
        0,
        `${project.key} is ${project.status} with ${present} artefacts present`,
      );
    }
  }
  for (const key of Object.keys(declared)) {
    assert(
      v2Projects.some((project) => project.key === key),
      `scripts/v2-slice-artefacts.json names unknown slice ${key}`,
    );
  }

  const seedKeys = new Set(seed.projects.map((project) => project.key));
  assert.deepEqual(
    [...projectKeys].sort(),
    [...seedKeys].sort(),
    "coverage and seed name different projects",
  );
  for (const project of seed.projects) {
    for (const dependency of project.meta.depends_on || []) {
      assert(seedKeys.has(dependency), `${project.key} depends on unknown ${dependency}`);
    }
  }
  assert.deepEqual(document.cycles, [], "the dependency graph has a cycle");

  const screenKeys = new Set();
  for (const screen of document.screens) {
    assert(!screenKeys.has(screen.key), `duplicate screen ${screen.key}`);
    screenKeys.add(screen.key);
    assert(
      SCREEN_STATUSES.has(screen.status),
      `${screen.key} has unknown screen status ${screen.status}`,
    );
    for (const evidence of screen.evidence) {
      assert(exists(evidence), `${screen.key} evidence does not resolve: ${evidence}`);
    }
    if (screen.status !== "live") {
      assert(screen.gap, `${screen.key} is ${screen.status} with no gap`);
    }
  }
  for (const key of pageKeys(v2)) {
    assert(screenKeys.has(key), `v2.js page ${key} has no coverage row`);
  }
  for (const key of screenKeys) {
    assert(pageKeys(v2).includes(key), `coverage screen ${key} is not a v2.js page`);
  }

  for (const invoked of document.actions_invoked) {
    assert(actions.has(invoked.name), `v2.js invokes unknown action ${invoked.name}`);
  }

  assert.deepEqual(
    document.summary.projects.uncovered,
    [],
    "a non-built project has no open unit",
  );
  assert.equal(
    document.summary.projects.total,
    document.projects.length,
    "summary project total",
  );
  assert.equal(document.summary.screens.total, document.screens.length, "summary screen total");
}

function load() {
  return {
    root,
    coverage: JSON.parse(read("docs/design/data/coverage.json")),
    actions: new Set(JSON.parse(read("docs/design/data/actions.json")).actions),
    issues: new Map(
      JSON.parse(read("docs/design/data/issues.json")).issues.map((issue) => [issue.number, issue]),
    ),
    seed: JSON.parse(read("seed/programme.json")),
    spec: read("docs/SPEC_V2.md"),
    v2: read("docs/design/v2.js"),
    v2art: JSON.parse(read("scripts/v2-slice-artefacts.json")).slices,
  };
}

function main() {
  const context = load();
  validate(context.coverage, context);
  const summary = context.coverage.summary;
  console.log(
    `check-coverage: ${summary.projects.total} projects ` +
      `(${JSON.stringify(summary.projects.by_status)}), ` +
      `${summary.screens.total} screens (${JSON.stringify(summary.screens.by_status)}), ` +
      `${context.coverage.actions_invoked.length} invoked actions resolve`,
  );

  if (process.argv.includes("--self-test")) {
    const refuse = (label, mutate) => {
      const copy = structuredClone(context.coverage);
      mutate(copy);
      assert.throws(() => validate(copy, context), undefined, label);
      console.log(`check-coverage: self-test refused ${label}`);
    };
    refuse("an unknown action", (copy) =>
      copy.actions_invoked.push({ name: "no.such_action", line: 1 }),
    );
    refuse("a missing page row", (copy) => {
      copy.screens = copy.screens.filter((screen) => screen.key !== "projects");
    });
    refuse("an uncovered project", (copy) => copy.summary.projects.uncovered.push("S99"));
    refuse("a missing evidence path", (copy) =>
      copy.projects[0].evidence.push("no/such/evidence.json"),
    );
    refuse("a V2 slice built with a missing artefact", (copy) => {
      const row = copy.projects.find((project) => project.key === "T0");
      assert(row, "self-test: T0 is not in coverage.json");
      row.measured.tables = [0, row.measured.tables[1]];
    });
    // #882: a curated behavioural slice cannot claim built without a delivered proof that the
    // filesystem resolves.
    {
      const ctx = { ...context, v2art: structuredClone(context.v2art) };
      const copy = structuredClone(context.coverage);
      copy.projects.find((project) => project.key === "T18").status = "built";
      delete ctx.v2art.T18.delivered;
      assert.throws(
        () => validate(copy, ctx),
        undefined,
        "a curated built slice without a delivered proof",
      );
      console.log(
        "check-coverage: self-test refused a curated built slice without a delivered proof",
      );
    }
    console.log("check-coverage: self-test 6/6 refusals");
  }
}

try {
  main();
} catch (error) {
  console.error("check-coverage: FAILED");
  console.error(error && error.stack ? error.stack : error);
  process.exit(1);
}
