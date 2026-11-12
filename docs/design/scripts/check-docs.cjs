#!/usr/bin/env node
/*
 * check-docs.cjs — the generated document pack.
 *
 * `build-docs.py` renders every in-scope Markdown file to a standalone HTML page and
 * writes the index (`documents.json` + `documents.js`). This check is the gate on that
 * output and needs neither pandoc nor a browser:
 *
 *   - every in-scope source is indexed exactly once, with a matching source sha256;
 *   - the committed output exists for every index entry;
 *   - documents.js and documents.json carry the same payload;
 *   - every local href/src in a generated page resolves, and no `.md` link remains;
 *   - no email, token, private IP or home path survived the render;
 *   - studio.html loads documents.js before docs.js before v2.js;
 *   - v2.js registers the destination and guards the feedback URL.
 *
 * Run: node docs/design/scripts/check-docs.cjs
 */
"use strict";

const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const { execFileSync } = require("node:child_process");

const root = path.resolve(__dirname, "../../..");
const pack = path.join(root, "docs", "design");
const out = path.join(pack, "docs");
const failures = [];
const checks = [];
const ok = (label) => checks.push(`ok   ${label}`);
const bad = (label) => {
  failures.push(label);
  checks.push(`FAIL ${label}`);
};

const HTTP = /^[a-z][a-z0-9+.-]*:/i;
const EMAIL = /[A-Za-z0-9._%+-]+@([A-Za-z0-9.-]+\.[A-Za-z]{2,})/g;
const TOKEN =
  /(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|sk-[A-Za-z0-9]{20,}|AKIA[0-9A-Z]{16}|xox[baprs]-[A-Za-z0-9\-]{10,}|npm_[A-Za-z0-9]{20,}|-----BEGIN [A-Z ]*PRIVATE KEY-----|(?i:bearer)\s+eyJ[A-Za-z0-9_\-]{20,}\.[A-Za-z0-9_\-]{20,}\.[A-Za-z0-9_\-]{20,})/;
const IPV4 = /(?:(?<![\d.])(?:\d{1,3}\.){3}\d{1,3}(?![\d.]))/g;
const HOME = /(?:\/Users\/[A-Za-z0-9._-]+|\/home\/[A-Za-z0-9._-]+)/g;
const SYNTHETIC_DOMAIN =
  /(?:^|\.)(?:example|invalid|test|localhost)$|^(?:acme\.dev|acme\.example|b\.co|request\.com|some\.host|other\.host|complex\.url\.com|pytest\.fixture|pytest\.mark\.sphinx|admin\.register|dev\.invalid|broken\.example|y\.example)$/i;
const SYNTHETIC_HOME = /^\/(?:home|Users)\/(?:tom|mulawa|workflows|user|alice|bob|wsykala|macbookpro)(?:\/|$)/;
const ALLOWED_IP = /^(?:0\.0\.0\.0|127\.0\.0\.1|255\.255\.255\.255|192\.0\.2\.\d{1,3}|198\.51\.100\.\d{1,3}|203\.0\.113\.\d{1,3})$/;
// The guard's hard set (scripts/ci/no-private-refs.sh, #54). The generator redacts
// these from every page; this is the assertion that it did.
const HARD_PRIVATE = [
  [/(?:^|[^A-Za-z0-9-])[A-Za-z0-9][A-Za-z0-9_-]*\.internal\b/, ".internal hostname"],
  [/(?:^|[^0-9.])100\.(?:6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7])\.[0-9]{1,3}\.[0-9]{1,3}/, "tailnet address"],
  [/\bts\.net\b/, "Tailscale name"],
  [/strato-(?:server|vps)/, "host login"],
  [/nonagenticai\/gitops-production/, "private CI repository path"],
];

const sha256 = (file) =>
  crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");

// Every in-scope Markdown source, discovered exactly as build-docs.py does.
const tracked = execFileSync("git", ["ls-files"], { cwd: root, encoding: "utf8" })
  .split("\n")
  .filter(Boolean);
const skip = [
  "docs/design/legacy/",
  "docs/design/docs/",
  "docs/design/scripts/",
  ".claude/",
  "crates/",
  "seed/",
  "scripts/",
  "parity/",
];
const sources = tracked.filter(
  (file) =>
    file.endsWith(".md") &&
    file !== "CLAUDE.md" &&
    (file === "README.md" || file.startsWith("docs/")) &&
    !skip.some((prefix) => file.startsWith(prefix)),
);

let index;
try {
  index = JSON.parse(fs.readFileSync(path.join(out, "documents.json"), "utf8"));
} catch (error) {
  bad(`documents.json does not parse: ${error.message}`);
}

if (index) {
  if (index.schema !== "design-docs/v1") bad(`schema is ${JSON.stringify(index.schema)}`);
  else ok("documents.json schema is design-docs/v1");
  if (Object.prototype.hasOwnProperty.call(index, "generated_at"))
    bad("documents.json carries generated_at (non-deterministic)");
  else ok("documents.json has no timestamp");
  if (!Array.isArray(index.documents)) bad("documents is not an array");
  else ok(`documents.json lists ${index.documents.length} documents`);

  let js;
  try {
    js = fs.readFileSync(path.join(out, "documents.js"), "utf8");
  } catch (error) {
    bad(`documents.js is missing: ${error.message}`);
  }
  if (js) {
    const prefix = "window.CONVEYOR_DOCUMENTS = ";
    if (!js.startsWith(prefix) || !js.trimEnd().endsWith(";")) {
      bad("documents.js is not a window.CONVEYOR_DOCUMENTS assignment");
    } else {
      try {
        const payload = JSON.parse(js.slice(prefix.length).replace(/;\s*$/, ""));
        if (JSON.stringify(payload) === JSON.stringify(index)) ok("documents.js matches documents.json");
        else bad("documents.js and documents.json differ");
      } catch (error) {
        bad(`documents.js payload does not parse: ${error.message}`);
      }
    }
  }

  const listed = new Map((index.documents || []).map((doc) => [doc.source, doc]));
  for (const source of sources) {
    const doc = listed.get(source);
    if (!doc) {
      bad(`source not indexed: ${source}`);
      continue;
    }
    if (!fs.existsSync(path.join(root, source))) bad(`source is absent: ${source}`);
    else if (sha256(path.join(root, source)) !== doc.sha256)
      bad(`source changed since index generation: ${source}`);
    if (!fs.existsSync(path.join(pack, doc.path))) bad(`generated page is absent: ${doc.path}`);
    if (String(doc.source || "").includes("legacy/")) bad(`retired study is indexed: ${doc.source}`);
  }
  for (const source of listed.keys()) {
    if (!sources.includes(source)) bad(`index lists an out-of-scope source: ${source}`);
  }
  if (!failures.length) ok("every in-scope source is indexed with a matching hash");
}

// Every local link resolves; no Markdown link survives; no private value survived.
if (index && Array.isArray(index.documents)) {
  let broken = 0;
  let leaked = 0;
  for (const doc of index.documents) {
    const file = path.join(pack, doc.path);
    if (!fs.existsSync(file)) continue;
    const html = fs.readFileSync(file, "utf8");
    for (const match of html.matchAll(/(?:href|src)="([^"]*)"/g)) {
      const url = match[1];
      if (!url || url.startsWith("#") || url.startsWith("//") || HTTP.test(url)) continue;
      const target = url.split("#")[0];
      if (!target) continue;
      if (/\.md$/i.test(target)) {
        bad(`${doc.path} still links to Markdown: ${url}`);
        broken += 1;
        continue;
      }
      let resolved;
      try {
        resolved = path.resolve(path.dirname(file), decodeURIComponent(target));
      } catch {
        resolved = path.resolve(path.dirname(file), target);
      }
      if (!fs.existsSync(resolved)) {
        bad(`${doc.path} has a broken link: ${url}`);
        broken += 1;
      }
    }
    for (const match of html.matchAll(EMAIL)) {
      if (SYNTHETIC_DOMAIN.test(match[1])) continue;
      bad(`${doc.path} carries an email address`);
      leaked += 1;
    }
    if (TOKEN.test(html)) {
      bad(`${doc.path} carries a credential-shaped string`);
      leaked += 1;
    }
    for (const match of html.matchAll(IPV4)) {
      const ip = match[0];
      const parts = ip.split(".").map(Number);
      if (parts.some((part) => part > 255) || ALLOWED_IP.test(ip)) continue;
      const isPrivate =
        parts[0] === 10 ||
        (parts[0] === 192 && parts[1] === 168) ||
        (parts[0] === 172 && parts[1] >= 16 && parts[1] <= 31) ||
        (parts[0] === 100 && parts[1] >= 64 && parts[1] <= 127);
      if (isPrivate) {
        bad(`${doc.path} carries a private IP address`);
        leaked += 1;
      }
    }
    for (const match of html.matchAll(HOME)) {
      if (SYNTHETIC_HOME.test(match[0])) continue;
      bad(`${doc.path} carries a home-directory path`);
      leaked += 1;
    }
    for (const [pattern, label] of HARD_PRIVATE) {
      if (pattern.test(html)) {
        bad(`${doc.path} carries a private ${label}`);
        leaked += 1;
      }
    }
  }
  for (const file of ["documents.json", "documents.js"]) {
    const text = fs.readFileSync(path.join(out, file), "utf8");
    for (const [pattern, label] of HARD_PRIVATE) {
      if (pattern.test(text)) {
        bad(`${file} carries a private ${label}`);
        leaked += 1;
      }
    }
  }
  if (!broken) ok("every local link in every generated page resolves");
  if (!leaked) ok("no email, token, private IP, home path or infrastructure identifier in any generated page");
}

// The pack wiring: scripts in order, destination registered, feedback URL guarded.
try {
  const studio = fs.readFileSync(path.join(pack, "studio.html"), "utf8");
  const at = (needle) => studio.indexOf(needle);
  const snapshot = at('src="data/snapshot.js"');
  const documents = at('src="docs/documents.js"');
  const docs = at('src="docs.js"');
  const v2 = at('src="v2.js"');
  if (snapshot === -1 || documents === -1 || docs === -1 || v2 === -1)
    bad("studio.html is missing one of snapshot/documents/docs/v2 scripts");
  else if (!(snapshot < documents && documents < docs && docs < v2))
    bad("studio.html must load data/snapshot.js, docs/documents.js, docs.js before v2.js");
  else ok("studio.html loads snapshot → documents → docs → v2 in order");
} catch (error) {
  bad(`studio.html does not read: ${error.message}`);
}

try {
  const v2 = fs.readFileSync(path.join(pack, "v2.js"), "utf8");
  if (!v2.includes('N("documents", "Documents"')) bad("v2.js has no Documents nav entry");
  else ok("v2.js registers the Documents nav entry");
  if (!v2.includes("documents: () => window.DocsPack.page(api)"))
    bad("v2.js has no documents page factory");
  else ok("v2.js registers the documents page");
  if (!v2.includes('documents: "Documents"')) bad("v2.js has no documents title");
  else ok("v2.js registers the documents title");
  if (!v2.includes("(live && live.feedback_url)"))
    bad("v2.js reportUrl still dereferences a null live snapshot");
  else ok("v2.js reportUrl guards a null snapshot");
} catch (error) {
  bad(`v2.js does not read: ${error.message}`);
}

for (const line of checks) console.log(line);
if (failures.length) {
  console.error(`\ncheck-docs: ${failures.length} failure(s)`);
  process.exit(1);
}
console.log(`\ncheck-docs: ${checks.length} checks passed`);
