#!/usr/bin/env node
/*
 * check-outputs.cjs — the published-output index and the pages it names agree.
 *
 * `docs/design/docs/outputs.js` is written by a console publish (`conveyor snapshot
 * --outputs-dir docs/design/docs`); on a checkout that has never published it is a placeholder
 * with no assignment, and no `output-*.html` pages exist. This check holds the pair:
 *
 *   - a placeholder index with committed pages is a regression (measured 2026-11-20: the T16
 *     merge deleted all 165 pages and restored the placeholder, and the design-pack job's
 *     conformance audit found it only indirectly);
 *   - every entry's `path` must resolve to a file under `docs/design/docs/`;
 *   - every committed `output-*.html` must be named by an entry.
 *
 * Run: node docs/design/scripts/check-outputs.cjs
 *      node docs/design/scripts/check-outputs.cjs --self-test
 */
"use strict";

const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..");
const docsDir = path.join(root, "docs");
const indexFile = path.join(docsDir, "outputs.js");

/**
 * Parse `window.CONVEYOR_OUTPUTS = {…};`, or null when the file is the placeholder.
 *
 * `//` comments are stripped first: the placeholder's comment names the assignment it describes,
 * and matching that text first made the placeholder itself unparsable — main's design-pack was red
 * from 8d39d7a3 until the committed index was restored. The last assignment wins, and its body runs
 * to the LAST `;` because the JSON may contain `;` inside a string (the published index does).
 */
function parseIndex(text) {
  const code = text.replace(/^\s*\/\/.*$/gm, "");
  const marker = "window.CONVEYOR_OUTPUTS";
  const at = code.lastIndexOf(marker);
  if (at === -1) return null;
  const equals = code.indexOf("=", at);
  if (equals === -1) return null;
  let body = code.slice(equals + 1);
  const end = body.lastIndexOf(";");
  if (end !== -1) body = body.slice(0, end);
  return JSON.parse(body.trim());
}

/** The failures for one index/pages pair. Pure, so the self-test can drive it. */
function evaluate(indexText, pageNames) {
  const failures = [];
  let index;
  try {
    index = parseIndex(indexText);
  } catch (error) {
    return [`outputs.js does not parse: ${error.message}`];
  }
  if (index === null) {
    if (pageNames.length > 0) {
      failures.push(
        `outputs.js has no CONVEYOR_OUTPUTS assignment while ${pageNames.length} published page(s) are committed`,
      );
    }
    return failures;
  }
  const outputs = Array.isArray(index.outputs) ? index.outputs : [];
  if (!Array.isArray(index.outputs)) failures.push("outputs.js has no outputs array");
  const named = new Set();
  for (const entry of outputs) {
    if (typeof entry.path !== "string" || !entry.path) {
      failures.push("an outputs entry has no path");
      continue;
    }
    // `<id>.path` is relative to the pack root (e.g. `docs/output-<id>.html` under
    // docs/design), not to the docs dir.
    const resolved = path.join(root, entry.path);
    named.add(path.basename(resolved));
    if (!fs.existsSync(resolved)) failures.push(`an outputs entry names a missing page: ${entry.path}`);
  }
  for (const name of pageNames) {
    if (!named.has(name)) failures.push(`a committed page has no outputs entry: ${name}`);
  }
  return failures;
}

function committedPages() {
  if (!fs.existsSync(docsDir)) return [];
  return fs
    .readdirSync(docsDir)
    .filter((name) => /^output-.*\.html$/.test(name))
    .sort();
}

function selfTest() {
  const cases = [];
  const expectFail = (label, text, pages) => {
    const failures = evaluate(text, pages);
    cases.push([`${label}: refused`, failures.length > 0]);
  };
  const expectPass = (label, text, pages) => {
    const failures = evaluate(text, pages);
    cases.push([`${label}: accepted`, failures.length === 0, failures.join("; ")]);
  };
  expectFail(
    "placeholder with committed pages",
    "// Placeholder. overwritten by a publish.\n",
    ["output-x.html"],
  );
  expectPass("placeholder and no pages", "// Placeholder. overwritten by a publish.\n", []);
  expectPass(
    "the placeholder whose comment names the assignment",
    "// Placeholder. `conveyor snapshot --outputs-dir docs/design/docs` overwrites this file with the\n" +
      '// published-output index (`window.CONVEYOR_OUTPUTS = {schema: "design-outputs/v1", …}`).\n' +
      "// A null payload means no platform output has been published.\n" +
      "window.CONVEYOR_OUTPUTS = null;\n",
    [],
  );
  expectFail(
    "an entry with no page",
    'window.CONVEYOR_OUTPUTS = {"outputs":[{"path":"docs/output-gone.html"}]};\n',
    [],
  );
  expectFail(
    "a page with no entry",
    'window.CONVEYOR_OUTPUTS = {"outputs":[]};\n',
    ["output-orphan.html"],
  );
  expectFail("an unparsable index", "window.CONVEYOR_OUTPUTS = nope;\n", []);
  let passed = 0;
  for (const [label, ok, detail] of cases) {
    if (ok) passed += 1;
    else console.log(`FAIL ${label}${detail ? ` (${detail})` : ""}`);
  }
  console.log(`check-outputs: self-test ${passed}/${cases.length} refusals and acceptances`);
  return passed === cases.length;
}

if (process.argv.includes("--self-test")) {
  process.exit(selfTest() ? 0 : 1);
}

const indexText = fs.existsSync(indexFile) ? fs.readFileSync(indexFile, "utf8") : "";
const failures = evaluate(indexText, committedPages());
for (const failure of failures) console.log(`FAIL ${failure}`);
console.log(
  failures.length === 0
    ? `check-outputs: ${committedPages().length} page(s) and the index agree`
    : `check-outputs: ${failures.length} failure(s)`,
);
process.exit(failures.length === 0 ? 0 : 1);
