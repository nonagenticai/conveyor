#!/usr/bin/env node
/*
 * check-standards.cjs — the generated MSP/TOGAF/SAFe/PMP artefact record.
 *
 * The record is a published artefact of the pack, not an optional side note: when
 * `data/standards.json` is absent the check fails, because a pack that ships the four-level view
 * without the artefact record is the exact defect this gate exists for. When the record is
 * published, the check is the gate on its honesty:
 *   - the schema is design-standards/v1 and the summary matches the artefacts;
 *   - every artefact's status is exactly the status of its sections;
 *   - every mandatory artefact that is not evidenced names a package that the published register
 *     actually carries (`package_published`), so an absent artefact is never a dead end;
 *   - every source in every section is a recognised kind and a resolving pointer;
 *   - a document-backed artefact has a page whose hash matches.
 *
 * Run: node docs/design/scripts/check-standards.cjs [--self-test]
 */
"use strict";

const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");

// The default is the published pack. The CI gate builds the record in a temp root instead, so
// `--data` names that record's directory and `--documents` the root its `docs/standards/` pages
// hang from; without them this validates exactly what the publish would serve.
const argv = process.argv.slice(2);
function option(name) {
  const index = argv.indexOf(name);
  return index >= 0 && index + 1 < argv.length ? argv[index + 1] : null;
}
const root = option("--documents") ? path.resolve(option("--documents")) : path.resolve(__dirname, "..");
const dataDir = option("--data") ? path.resolve(option("--data")) : path.join(root, "data");
const dataPath = path.join(dataDir, "standards.json");
const STATUSES = new Set(["evidenced", "partial", "absent"]);
const STANDARDS = new Set(["MSP", "TOGAF", "SAFe", "PMP", "SPEC"]);
const LEVELS = new Set(["portfolio", "programme", "project", "work_package"]);
const SUBJECTS = new Set(["programme", "project", "tranche", "benefit", "capability", "phase", "card"]);
const SOURCE_KINDS = new Set(["rows", "documents", "computed", "register"]);

function validate(record, documentsRoot, registerKeys) {
  const errors = [];
  if (!record || typeof record !== "object") return ["the record is not an object"];
  if (record.schema !== "design-standards/v1") {
    errors.push(`schema is ${JSON.stringify(record.schema)}, expected "design-standards/v1"`);
  }
  const artefacts = Array.isArray(record.artefacts) ? record.artefacts : [];
  if (!artefacts.length) errors.push("artefacts is empty");
  const ids = new Set();
  const counts = { evidenced: 0, partial: 0, absent: 0 };
  for (const artefact of artefacts) {
    const id = String(artefact.id || "?");
    if (ids.has(id)) errors.push(`${id}: repeated id`);
    ids.add(id);
    if (!STANDARDS.has(artefact.standard)) errors.push(`${id}: unknown standard ${artefact.standard}`);
    if (!LEVELS.has(artefact.level)) errors.push(`${id}: unknown level ${artefact.level}`);
    if (!SUBJECTS.has(artefact.subject_kind)) errors.push(`${id}: unknown subject kind ${artefact.subject_kind}`);
    if (!STATUSES.has(artefact.status)) {
      errors.push(`${id}: unknown status ${artefact.status}`);
      continue;
    }
    counts[artefact.status] += 1;
    const sections = Array.isArray(artefact.sections) ? artefact.sections : [];
    if (!sections.length) errors.push(`${id}: no sections`);
    const evidenced = sections.filter((section) => section.status === "evidenced").length;
    const expected =
      evidenced === sections.length ? "evidenced" : evidenced ? "partial" : "absent";
    if (artefact.status !== expected) {
      errors.push(`${id}: status ${artefact.status} but sections say ${expected}`);
    }
    for (const section of sections) {
      if (!section.heading) errors.push(`${id}: a section has no heading`);
      if (!STATUSES.has(section.status)) errors.push(`${id}: section ${section.heading} has no status`);
      if (
        section.status === "evidenced" &&
        !(section.sources || []).some((source) => Number(source.count) > 0)
      ) {
        errors.push(`${id}: section ${section.heading} is evidenced with no source`);
      }
      for (const source of section.sources || []) {
        if (!SOURCE_KINDS.has(source.kind)) {
          errors.push(`${id}: section ${section.heading} has source kind ${source.kind}`);
        }
        if ((source.kind === "rows" || source.kind === "documents") && !String(source.pointer || "").startsWith("/")) {
          errors.push(`${id}: section ${section.heading} has no JSON pointer`);
        }
      }
    }
    if (artefact.mandatory && artefact.status !== "evidenced") {
      if (!artefact.package_key) errors.push(`${id}: ${artefact.status} and no package key`);
      if (artefact.package_published !== true) {
        errors.push(`${id}: ${artefact.status} but its package is not in the published register`);
      } else if (registerKeys && !registerKeys.has(artefact.package_key)) {
        errors.push(`${id}: package ${artefact.package_key} is not in data/rework.json`);
      }
    }
    const document = artefact.document;
    if (!document || !document.path) {
      errors.push(`${id}: no document path`);
    } else {
      const page = path.join(documentsRoot, document.path);
      if (!fs.existsSync(page)) {
        errors.push(`${id}: page ${document.path} is absent`);
      } else {
        const sha = crypto.createHash("sha256").update(fs.readFileSync(page)).digest("hex");
        if (sha !== document.sha256) errors.push(`${id}: page hash does not match`);
      }
    }
  }
  for (const status of Object.keys(counts)) {
    const declared = (record.summary || {})[status];
    if (declared !== counts[status]) {
      errors.push(`summary ${status} ${declared} != ${counts[status]}`);
    }
  }
  const byStandard = record.by_standard || {};
  const seen = new Set();
  for (const artefact of artefacts) seen.add(artefact.standard);
  for (const standard of seen) {
    const row = byStandard[standard] || {};
    const actual = artefacts.filter((artefact) => artefact.standard === standard);
    if (row.artefacts !== actual.length) {
      errors.push(`by_standard ${standard} ${row.artefacts} != ${actual.length}`);
    }
    for (const status of Object.keys(counts)) {
      const measured = actual.filter((artefact) => artefact.status === status).length;
      if ((row[status] || 0) !== measured) {
        errors.push(`by_standard ${standard} ${status} ${row[status] || 0} != ${measured}`);
      }
    }
  }
  return errors;
}

function selfTest() {
  const tmp = fs.mkdtempSync(path.join(require("node:os").tmpdir(), "standards-check-"));
  fs.mkdirSync(path.join(tmp, "data"), { recursive: true });
  const pageRel = "data/page.html";
  const pageBody = "<!doctype html><h1>fixture</h1>";
  fs.writeFileSync(path.join(tmp, pageRel), pageBody);
  const pageSha = crypto.createHash("sha256").update(pageBody).digest("hex");
  const base = () => ({
    schema: "design-standards/v1",
    summary: { artefacts: 1, mandatory: 1, evidenced: 0, partial: 0, absent: 1, findings: 1 },
    by_standard: { MSP: { artefacts: 1, evidenced: 0, partial: 0, absent: 1 } },
    artefacts: [
      {
        id: "std-msp-vision-x",
        standard: "MSP",
        level: "programme",
        subject_kind: "programme",
        category: "vision",
        mandatory: true,
        status: "absent",
        package_key: "std-msp-vision-x:programme",
        package_published: true,
        sections: [{ heading: "Purpose", status: "absent", sources: [{ kind: "rows", pointer: "/x", count: 0 }] }],
        document: { path: pageRel, sha256: pageSha },
      },
    ],
  });
  const cases = [];
  cases.push(["a consistent absent artefact", base(), true]);
  const inconsistent = base();
  inconsistent.artefacts[0].status = "evidenced";
  cases.push(["a status that disagrees with its sections", inconsistent, false]);
  const unlinked = base();
  unlinked.artefacts[0].package_published = false;
  cases.push(["a mandatory absent artefact with no published package", unlinked, false]);
  const badSource = base();
  badSource.artefacts[0].sections[0].sources = [{ kind: "not-a-kind", pointer: "/x", count: 0 }];
  cases.push(["an unknown source kind", badSource, false]);
  let failures = 0;
  for (const [label, candidate, expectValid] of cases) {
    const errors = validate(candidate, tmp);
    const valid = errors.length === 0;
    if (valid !== expectValid) {
      failures += 1;
      console.log(`check-standards: self-test FAILED to refuse ${label}`);
    } else {
      console.log(`check-standards: self-test ${expectValid ? "accepted" : "refused"} ${label}`);
    }
  }
  fs.rmSync(tmp, { recursive: true, force: true });
  if (failures) return 1;
  console.log(`check-standards: self-test ${cases.length}/${cases.length}`);
  return 0;
}

function main() {
  if (process.argv.includes("--self-test")) return selfTest();
  if (!fs.existsSync(dataPath)) {
    console.log(
      `check-standards: no standards record at ${dataPath}; ` +
        "run docs/design/scripts/publish-standards.sh and commit the pack",
    );
    return 1;
  }
  let record;
  try {
    record = JSON.parse(fs.readFileSync(dataPath, "utf8"));
  } catch (error) {
    console.log(`check-standards: standards.json does not parse: ${error.message}`);
    return 1;
  }
  let registerKeys = null;
  const reworkPath = path.join(dataDir, "rework.json");
  const reworkJsPath = path.join(dataDir, "rework.js");
  let register = null;
  if (fs.existsSync(reworkPath)) {
    try {
      register = JSON.parse(fs.readFileSync(reworkPath, "utf8"));
    } catch (error) {
      console.log(`check-standards: data/rework.json does not parse: ${error.message}`);
      return 1;
    }
  } else if (fs.existsSync(reworkJsPath)) {
    try {
      register = JSON.parse(
        fs
          .readFileSync(reworkJsPath, "utf8")
          .replace(/^window\.CONVEYOR_REWORK = /, "")
          .replace(/;\s*$/, ""),
      );
    } catch (error) {
      console.log(`check-standards: data/rework.js does not parse: ${error.message}`);
      return 1;
    }
  }
  if (register) {
    registerKeys = new Set((register.packages || []).map((pkg) => String(pkg.key)));
  }
  const errors = validate(record, root, registerKeys);
  if (errors.length) {
    for (const error of errors) console.log(`check-standards: ERROR: ${error}`);
    return 1;
  }
  console.log(
    `check-standards: ${record.summary.artefacts} artefacts, ${record.summary.evidenced} evidenced, ` +
      `${record.summary.partial} partial, ${record.summary.absent} absent`,
  );
  return 0;
}

process.exit(main());
