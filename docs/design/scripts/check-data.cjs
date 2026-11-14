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

const organisationPath = path.join(dataDir, "organisation.json");
if (fs.existsSync(organisationPath)) {
  let organisation;
  try {
    organisation = JSON.parse(fs.readFileSync(organisationPath, "utf8"));
  } catch (error) {
    bad(`organisation.json does not parse: ${error.message}`);
  }
  if (organisation) {
    const arrays = ["industries", "departments", "jobs", "releases", "bindings"];
    const missing = arrays.filter((key) => !Array.isArray(organisation[key]));
    if (missing.length) {
      bad(`organisation.json is missing array(s): ${missing.join(", ")}`);
    } else {
      const { industries, departments, jobs, releases, bindings } = organisation;
      const jobIds = new Set(jobs.map((job) => job.id));
      const releaseHashes = new Set(releases.map((release) => release.release_hash));
      const badStatus = (row, allowed) => !allowed.includes(row.status);
      const jobProblems = jobs.filter(
        (job) => !job.key || !job.name || badStatus(job, ["draft", "active", "retired"]),
      );
      const releaseProblems = releases.filter(
        (release) =>
          !/^[0-9a-f]{64}$/.test(String(release.release_hash || "")) ||
          !jobIds.has(release.job_id) ||
          badStatus(release, ["published", "retired"]),
      );
      const bindingProblems = bindings.filter(
        (binding) =>
          !binding.executor_kind ||
          !jobIds.has(binding.job_id) ||
          !releaseHashes.has(binding.release_hash) ||
          badStatus(binding, ["draft", "active", "retired"]),
      );
      if (jobProblems.length) bad(`${jobProblems.length} job row(s) are missing a key/title/status`);
      if (releaseProblems.length) bad(`${releaseProblems.length} release row(s) are malformed or orphaned`);
      if (bindingProblems.length) bad(`${bindingProblems.length} binding row(s) are malformed or orphaned`);
      if (!jobProblems.length && !releaseProblems.length && !bindingProblems.length) {
        ok(
          `organisation: ${industries.length} industries / ${departments.length} departments / ` +
            `${jobs.length} jobs / ${releases.length} releases / ${bindings.length} bindings`,
        );
      }
    }
  }
} else {
  bad("organisation.json is absent while data/ exists");
}

const studioPath = path.join(root, "studio.html");
if (fs.existsSync(studioPath)) {
  const html = fs.readFileSync(studioPath, "utf8");
  const dataAt = html.indexOf('src="data/snapshot.js"');
  const v2At = html.indexOf('src="v2.js"');
  if (dataAt === -1) {
    bad("studio.html does not load data/snapshot.js");
  } else if (v2At === -1 || dataAt > v2At) {
    bad("studio.html must load data/snapshot.js before v2.js");
  } else {
    ok("studio.html loads data/snapshot.js before v2.js");
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
