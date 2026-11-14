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
 *   - every project's units exist in the issue registry;
 *   - the seed and the coverage agree on the project set, and no dependency dangles;
 *   - no non-built project is without an open unit.
 *
 * `--self-test` proves the gate refuses: a bad action, a missing page row, an uncovered
 * project and a missing evidence path each exit non-zero.
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
    console.log("check-coverage: self-test 4/4 refusals");
  }
}

try {
  main();
} catch (error) {
  console.error("check-coverage: FAILED");
  console.error(error && error.stack ? error.stack : error);
  process.exit(1);
}
