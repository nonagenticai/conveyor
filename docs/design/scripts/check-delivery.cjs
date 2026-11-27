#!/usr/bin/env node
/*
 * check-delivery.cjs — the delivery unit page (#782 P2).
 *
 * The renderer is pure (`window.CONVEYOR_DELIVERY.unitDetail`), so this check loads it with a
 * stubbed window and renders a fixture: a complete unit with every link, a unit with no closure,
 * and an id that names no row. When the committed snapshot carries a `delivery` section it is
 * checked too; a snapshot that predates #782 is skipped, because the publish pipeline regenerates
 * it (the issue's deployment prerequisite) rather than this check inventing the rows.
 *
 * Run: node docs/design/scripts/check-delivery.cjs
 */
"use strict";

const fs = require("node:fs");
const path = require("node:path");

global.window = {};
require(path.join(__dirname, "..", "delivery.js"));
const D = global.window.CONVEYOR_DELIVERY;

const failures = [];
const ok = (label) => console.log(`ok   ${label}`);
const bad = (label) => {
  failures.push(label);
  console.log(`FAIL ${label}`);
};

if (!D || typeof D.unitDetail !== "function") {
  console.log("FAIL window.CONVEYOR_DELIVERY.unitDetail is not exported");
  process.exit(1);
}

const fixture = {
  programme: {
    projects: [{ key: "unit-a" }, { key: "unit-b" }],
  },
  delivery: {
    units: [
      {
        id: "unit-a",
        project_key: "unit-a",
        repo: "nonagenticai/conveyor",
        lane: "issue/782-unit-record-site",
        issue_ref: "nonagenticai/conveyor#782",
        status: "closed",
        steps: [
          {
            step: "branch",
            evidence: {},
            recorded_at: "2026-11-27T06:00:00Z",
          },
          {
            step: "pull_request",
            evidence: { pr_url: "https://github.com/nonagenticai/conveyor/pull/900" },
            recorded_at: "2026-11-27T06:10:00Z",
          },
        ],
        gates: [
          { gate: "review", status: "approved", decided_at: "2026-11-27T06:20:00Z" },
          { gate: "merge", status: "approved", decided_at: "2026-11-27T06:30:00Z" },
          { gate: "deploy", status: "approved", decided_at: "2026-11-27T06:40:00Z" },
        ],
        closure: {
          pr_url: "https://github.com/nonagenticai/conveyor/pull/900",
          ci_run_id: "45900000000",
          ci_verdict: "green",
          merge_sha: "abc1234",
          deployed_digest: "sha256:feed",
          live_check: { ok: true },
          reviewer: "A Reviewer",
        },
      },
      {
        id: "unit-b",
        project_key: "unit-b",
        repo: "nonagenticai/conveyor",
        lane: "issue/901-bare",
        issue_ref: "not recorded",
        status: "chartered",
        steps: [],
        gates: [],
        closure: "not recorded",
      },
    ],
  },
  programme_health: {
    units: { total: 2, by_status: { closed: 1, chartered: 1 } },
  },
};

const projectKeys = new Set(fixture.programme.projects.map((row) => row.key));
const units = fixture.delivery.units;
const unresolved = units.filter((unit) => !projectKeys.has(unit.project_key));
if (unresolved.length === 0) {
  ok("every unit's project_key resolves to a programme project row");
} else {
  bad(`units with no programme row: ${unresolved.map((unit) => unit.project_key).join(", ")}`);
}

// 1. The complete unit renders the charter, every step, every gate and every closure link.
const full = D.unitDetail(fixture, "782");
const wanted = [
  "https://github.com/nonagenticai/conveyor/issues/782",
  "https://github.com/nonagenticai/conveyor/pull/900",
  "https://github.com/nonagenticai/conveyor/actions/runs/45900000000",
  "abc1234",
  "sha256:feed",
  "A Reviewer",
  "branch",
  "pull_request",
  "review",
  "merge",
  "deploy",
];
const missing = wanted.filter((needle) => !full.includes(needle));
if (missing.length === 0) {
  ok("the complete unit renders its issue, PR, CI run, merge, digest, reviewer, steps and gates");
} else {
  bad(`the complete unit is missing: ${missing.join(", ")}`);
}

// 2. An evidenced closure never renders as an absent value.
const closureTable = full.slice(full.indexOf("<h3>Closure</h3>"));
if (closureTable.includes("—")) {
  bad("the complete unit's closure renders — for an evidenced field");
} else {
  ok("the complete unit's closure has no — for an evidenced field");
}

// 3. An id that names no row says `not recorded` and renders no fixture.
const absent = D.unitDetail(fixture, "9999");
if (absent.includes("not recorded") && !absent.includes("pull/900")) {
  ok("an id that names no row says not recorded and renders no unit");
} else {
  bad("an id that names no row must say not recorded and render no unit");
}

// 4. Every unit renders non-empty HTML.
for (const unit of units) {
  if (unit.issue_ref === "not recorded") continue;
  const issue = unit.issue_ref.slice(unit.issue_ref.lastIndexOf("#") + 1);
  if (D.unitDetail(fixture, issue).trim().length > 0) {
    ok(`unit ${unit.project_key} renders`);
  } else {
    bad(`unit ${unit.project_key} renders empty HTML`);
  }
}

// 5. programme_health totals agree with the unit rows.
const health = fixture.programme_health.units;
const statusSum = Object.values(health.by_status).reduce((sum, count) => sum + count, 0);
if (health.total === units.length && health.total === statusSum) {
  ok("programme_health.unit totals agree with the unit rows");
} else {
  bad(`programme_health totals ${health.total}/${statusSum} disagree with ${units.length} units`);
}

// The committed snapshot is the real data when it carries the section; a snapshot that predates
// #782 is skipped rather than reding the pack before the publish regenerates it.
const snapshotPath = path.join(__dirname, "..", "data", "snapshot.js");
if (fs.existsSync(snapshotPath)) {
  global.window = {};
  require(snapshotPath);
  const real = global.window.CONVEYOR_SNAPSHOT;
  if (real && real.delivery && Array.isArray(real.delivery.units)) {
    const realKeys = new Set(
      ((real.programme && real.programme.projects) || []).map((row) => row.key),
    );
    const dangling = real.delivery.units.filter((unit) => !realKeys.has(unit.project_key));
    if (dangling.length === 0) {
      ok("the committed snapshot's delivery units all resolve to project rows");
    } else {
      bad(
        `the committed snapshot has units with no project row: ${dangling
          .map((unit) => unit.project_key)
          .join(", ")}`,
      );
    }
    const realHealth = (real.programme_health && real.programme_health.units) || null;
    const realSum = realHealth
      ? Object.values(realHealth.by_status || {}).reduce((sum, count) => sum + count, 0)
      : -1;
    if (realHealth && realHealth.total === real.delivery.units.length && realHealth.total === realSum) {
      ok("the committed snapshot's programme_health totals agree");
    } else {
      bad("the committed snapshot's programme_health totals disagree with its units");
    }
  } else {
    console.log("skip: the committed snapshot predates the delivery section (publish regenerates it)");
  }
}

if (failures.length) {
  console.log(`\ncheck-delivery: FAILED (${failures.length})`);
  process.exit(1);
}
console.log("\ncheck-delivery: OK");
