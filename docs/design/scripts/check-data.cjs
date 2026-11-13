#!/usr/bin/env node
/*
 * check-data.cjs — the generated snapshot beside the studio pack.
 *
 * A clean checkout has no `data/` directory: that is DESIGN mode and this check is a no-op.
 * When a snapshot IS present, the check is the gate on it:
 *   - manifest.json parses and names every file with a sha256;
 *   - every listed file exists and its bytes hash to the manifest value;
 *   - snapshot.js is a valid `window.CONVEYOR_SNAPSHOT = {…};` assignment;
 *   - secrets.json contains metadata only (no value/ciphertext/key id);
 *   - no file carries an email or a credential-shaped string;
 *   - studio.html loads data/snapshot.js before v2.js.
 *
 * Run: node docs/design/scripts/check-data.cjs
 */
"use strict";

const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..");
const dataDir = path.join(root, "data");
const failures = [];
const checks = [];
const ok = (label) => checks.push(`ok   ${label}`);
const bad = (label) => {
  failures.push(label);
  checks.push(`FAIL ${label}`);
};

const EMAIL = /[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}/;
const TOKEN =
  /(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|sk-[A-Za-z0-9]{20,}|AKIA[0-9A-Z]{16}|xox[baprs]-[A-Za-z0-9\-]{10,}|-----BEGIN [A-Z ]*PRIVATE KEY-----)/;
const IP = /\b(?:\d{1,3}\.){3}\d{1,3}\b/;
const HOME_PATH = /(?:~\/|\/Users\/[A-Za-z0-9._-]+|\/home\/[A-Za-z0-9._-]+)/;

if (!fs.existsSync(dataDir)) {
  console.log("check-data: no data/ directory — DESIGN mode (nothing to verify)");
  process.exit(0);
}

// The plan seed is committed beside the snapshot (not a snapshot output), so it is checked in
// both modes: it must be a valid assignment and it must carry no private value.
const planSeedPath = path.join(dataDir, "plan-seed.js");
if (fs.existsSync(planSeedPath)) {
  const text = fs.readFileSync(planSeedPath, "utf8");
  try {
    const json = text
      .replace(/^[\s\S]*?window\.CONVEYOR_PLAN_SEED = /, "")
      .replace(/;\s*$/, "");
    const value = JSON.parse(json);
    if (value.schema !== "design-plan-seed/v1" || !Array.isArray(value.projects)) {
      bad("plan-seed.js payload has the wrong schema");
    } else {
      ok("plan-seed.js is a valid plan-seed assignment");
    }
  } catch (error) {
    bad(`plan-seed.js payload does not parse: ${error.message}`);
  }
  if (EMAIL.test(text)) bad("plan-seed.js carries an email address");
  if (TOKEN.test(text)) bad("plan-seed.js carries a credential-shaped string");
  if (IP.test(text)) bad("plan-seed.js carries an IP address");
  if (HOME_PATH.test(text)) bad("plan-seed.js carries a home-directory path");
  if (!EMAIL.test(text) && !TOKEN.test(text) && !IP.test(text) && !HOME_PATH.test(text)) {
    ok("plan-seed.js carries no email, credential, IP or home path");
  }
} else {
  bad("plan-seed.js is absent");
}
// The DESIGN-mode early exit below must not swallow a plan-seed failure.
if (failures.length) report();

const manifestPath = path.join(dataDir, "manifest.json");
const snapshotJsPath = path.join(dataDir, "snapshot.js");
if (!fs.existsSync(manifestPath)) {
  // `data/issues.json` and a null-payload `snapshot.js` are the committed pre-publish state.
  // A snapshot.js with a payload but no manifest is a broken publish.
  const stub =
    fs.existsSync(snapshotJsPath) &&
    /window\.CONVEYOR_SNAPSHOT = null;\s*$/.test(fs.readFileSync(snapshotJsPath, "utf8"));
  if (!fs.existsSync(snapshotJsPath) || stub) {
    console.log("check-data: no snapshot yet — DESIGN mode (nothing to verify)");
    process.exit(0);
  }
  bad("data/snapshot.js carries a payload but manifest.json is absent");
  report();
}

let manifest;
try {
  manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
} catch (error) {
  bad(`manifest.json does not parse: ${error.message}`);
  report();
}

if (manifest.schema !== "design-snapshot/v1") {
  bad(`manifest.schema is ${JSON.stringify(manifest.schema)}, expected "design-snapshot/v1"`);
} else {
  ok("manifest schema is design-snapshot/v1");
}
if (!Array.isArray(manifest.files) || manifest.files.length === 0) {
  bad("manifest.files is empty");
} else {
  ok(`manifest lists ${manifest.files.length} files`);
}

for (const file of manifest.files || []) {
  const target = path.join(dataDir, file.path);
  if (!fs.existsSync(target)) {
    bad(`${file.path} is listed but absent`);
    continue;
  }
  const bytes = fs.readFileSync(target);
  const digest = crypto.createHash("sha256").update(bytes).digest("hex");
  if (digest !== file.sha256) {
    bad(`${file.path} hash ${digest} != manifest ${file.sha256}`);
  } else if (bytes.length !== file.bytes) {
    bad(`${file.path} size ${bytes.length} != manifest ${file.bytes}`);
  } else {
    ok(`${file.path} matches its manifest hash (${file.bytes} bytes)`);
  }
}

const snapshotJs = path.join(dataDir, "snapshot.js");
if (!fs.existsSync(snapshotJs)) {
  bad("snapshot.js is absent");
} else {
  const text = fs.readFileSync(snapshotJs, "utf8");
  if (!text.startsWith("window.CONVEYOR_SNAPSHOT = ") || !text.trimEnd().endsWith(";")) {
    bad("snapshot.js is not a window.CONVEYOR_SNAPSHOT assignment");
  } else {
    try {
      const json = text
        .replace(/^window\.CONVEYOR_SNAPSHOT = /, "")
        .replace(/;\s*$/, "");
      const value = JSON.parse(json);
      if (value.schema !== "design-snapshot/v1") {
        bad("snapshot.js payload has the wrong schema");
      } else {
        ok("snapshot.js is a valid snapshot assignment");
      }
    } catch (error) {
      bad(`snapshot.js payload does not parse: ${error.message}`);
    }
  }
}

const secretsPath = path.join(dataDir, "secrets.json");
if (fs.existsSync(secretsPath)) {
  const secrets = JSON.parse(fs.readFileSync(secretsPath, "utf8"));
  const allowed = new Set(["count", "scopes", "last_rotated_at"]);
  let stray = 0;
  for (const key of Object.keys(secrets.secrets || {})) {
    if (!allowed.has(key)) {
      stray += 1;
      bad(`secrets.json carries a forbidden field ${key}`);
    }
  }
  if (stray === 0) ok("secrets.json carries count/scope metadata only");
} else {
  bad("secrets.json is absent while data/ exists");
}

// Every published string is scanned, not just the card bodies.
for (const file of manifest.files || []) {
  const target = path.join(dataDir, file.path);
  if (!fs.existsSync(target)) continue;
  const text = fs.readFileSync(target, "utf8");
  if (EMAIL.test(text)) bad(`${file.path} carries an email address`);
  if (TOKEN.test(text)) bad(`${file.path} carries a credential-shaped string`);
  if (IP.test(text)) bad(`${file.path} carries an IP address`);
  if (HOME_PATH.test(text)) bad(`${file.path} carries a home-directory path`);
}
if (failures.length === 0) ok("no email, credential, IP or home path in any file");

const issuesPath = path.join(dataDir, "issues.json");
if (fs.existsSync(issuesPath)) {
  let issues;
  try {
    issues = JSON.parse(fs.readFileSync(issuesPath, "utf8"));
  } catch (error) {
    bad(`issues.json does not parse: ${error.message}`);
  }
  let leaked = 0;
  for (const issue of (issues && issues.issues) || []) {
    for (const forbidden of ["body", "author"]) {
      if (Object.prototype.hasOwnProperty.call(issue, forbidden)) {
        leaked += 1;
        bad(`issues.json carries a ${forbidden} for #${issue.number}`);
      }
    }
    const title = String(issue.title || "");
    if (EMAIL.test(title) || TOKEN.test(title) || IP.test(title) || HOME_PATH.test(title)) {
      bad(`issues.json title #${issue.number} is not redacted`);
    }
  }
  if (leaked === 0) ok("issues.json carries no bodies or authors");
}

const studioPath = path.join(root, "studio.html");
if (fs.existsSync(studioPath)) {
  const html = fs.readFileSync(studioPath, "utf8");
  const dataAt = html.indexOf('src="data/snapshot.js"');
  const planAt = html.indexOf('src="data/plan-seed.js"');
  const v2At = html.indexOf('src="v2.js"');
  if (dataAt === -1) {
    bad("studio.html does not load data/snapshot.js");
  } else if (v2At === -1 || dataAt > v2At) {
    bad("studio.html must load data/snapshot.js before v2.js");
  } else {
    ok("studio.html loads data/snapshot.js before v2.js");
  }
  if (planAt === -1) {
    bad("studio.html does not load data/plan-seed.js");
  } else if (v2At === -1 || planAt > v2At) {
    bad("studio.html must load data/plan-seed.js before v2.js");
  } else {
    ok("studio.html loads data/plan-seed.js before v2.js");
  }
}

report();

function report() {
  for (const line of checks) console.log(line);
  if (failures.length) {
    console.error(`\ncheck-data: ${failures.length} failure(s)`);
    process.exit(1);
  }
  console.log(`\ncheck-data: ${checks.length} checks passed`);
}
