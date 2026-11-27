import assert from "node:assert/strict";
import test from "node:test";

import { matchPath, resolve, routes } from "./registry.js";
import core from "./routes/core.js";

test("core owns the entrance and the two Portfolio rows, host-rooted", () => {
  // The registry composes core first, then the wings' files in a fixed order. Assert the core
  // prefix, not the whole list: a wing's rows are composed after core and must not fail the
  // foundation's own test.
  const coreWalks = core.map((row) => row.walk);
  assert.deepEqual(
    routes.slice(0, coreWalks.length).map((row) => row.walk),
    coreWalks,
  );
  for (const row of routes) assert.equal(row.walk, row.app);
});

test("the composed walks are unique", () => {
  const walks = routes.map((row) => row.walk);
  assert.equal(new Set(walks).size, walks.length);
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
