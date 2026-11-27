import assert from "node:assert/strict";
import test from "node:test";

import { matchPath, resolve, routes } from "./registry.js";

test("the registry composes core first, then the wings, with unique walk patterns", () => {
  const walks = routes.map((row) => row.walk);
  assert.deepEqual(walks.slice(0, 3), ["/", "/portfolio", "/portfolio/:programme_id"]);
  for (const walk of ["/procedures", "/build/processes", "/build/process-map", "/authoring", "/design/adm", "/architecture", "/design/requirements", "/design/models", "/copilot", "/approvals", "/evals", "/evals/benchmarks", "/evals/benchmarks/run/:run_id", "/evals/improvement", "/evals/:eval_id", "/evals/:eval_id/:tab", "/improve", "/knowledge", "/measure/prompt-audit", "/measure/usage", "/metering", "/operations", "/platform", "/search", "/settings/admin", "/settings/inference", "/settings/members", "/settings/roles"]) {
    assert.ok(walks.includes(walk), `missing ${walk}`);
  }
  assert.equal(new Set(walks).size, walks.length, "duplicate walk pattern");
  for (const row of routes) assert.equal(row.walk, row.app);
});

test("a literal path matches itself", () => {
  assert.deepEqual(matchPath("/portfolio", "/portfolio"), {});
  assert.equal(matchPath("/portfolio", "/board"), null);
  assert.deepEqual(matchPath("/", "/"), {});
});

test("a parameter segment captures one segment", () => {
  assert.deepEqual(matchPath("/portfolio/:programme_id", "/portfolio/01a1"), {
    programme_id: "01a1",
  });
  assert.equal(matchPath("/portfolio/:programme_id", "/portfolio"), null);
  assert.equal(matchPath("/portfolio/:programme_id", "/portfolio/a/b"), null);
});

test("resolve returns the row and the params", () => {
  const found = resolve("/portfolio/abc");
  assert.ok(found);
  assert.equal(found.row.room, "portfolio");
  assert.equal(found.params.programme_id, "abc");
  assert.equal(resolve("/nope"), null);
});
