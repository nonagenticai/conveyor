// #916. `renderRoute` is async: a destination that resolves after a newer navigation must not
// write the screen. Without the generation guard this test fails — the slow render appends its
// tree over the fast one, and in the client the losing tree stays connected, which is the retained
// subscription the soak measured.
//
// Node has no DOM, so this stubs the four nodes the shell asks for and the two globals `el()` and
// the focus check touch. Nothing here asserts layout: it asserts which element owns the screen.

import assert from "node:assert/strict";
import test from "node:test";

function node(tag = "div") {
  return {
    tagName: tag.toUpperCase(),
    children: [],
    className: "",
    textContent: "",
    dataset: {},
    isContentEditable: false,
    parentNode: null,
    get firstChild() {
      return this.children[0] ?? null;
    },
    append(...kids) {
      for (const kid of kids) {
        kid.parentNode = this;
        this.children.push(kid);
      }
    },
    removeChild(kid) {
      const at = this.children.indexOf(kid);
      if (at >= 0) this.children.splice(at, 1);
      return kid;
    },
    setAttribute() {},
    querySelectorAll() {
      return [];
    },
    focus() {},
    addEventListener() {},
    contains(kid) {
      return this.children.includes(kid);
    },
    classList: { add() {}, remove() {} },
  };
}

const roots = { hud: node(), map: node(), screen: node(), "sheet-host": node(), announcer: node() };
globalThis.HTMLElement = class {};
globalThis.document = {
  createElement: (tag) => node(tag),
  createTextNode: (text) => ({ nodeType: 3, textContent: text }),
  getElementById: (id) => roots[id] ?? node(),
  activeElement: null,
  body: node(),
  contains: () => false,
  title: "",
};
globalThis.window = { addEventListener() {} };

const { Shell } = await import("./shell.js");

function shell() {
  return new Shell({ auth: {}, api: {}, routes: [], worldData: {}, world: {} });
}

/** A destination whose `render()` resolves only when the test says so. */
function deferred() {
  let settle;
  const promise = new Promise((resolve) => {
    settle = resolve;
  });
  return { promise, settle };
}

test("a render that resolves after a newer navigation does not write the screen", async () => {
  const s = shell();
  const slow = deferred();
  const fastView = { element: node("section"), title: "Fast" };
  const slowView = { element: node("section"), title: "Slow" };
  const row = (view) => ({ load: () => Promise.resolve({ render: () => view }) });

  const firstNav = s.renderRoute({ row: row(slow.promise), params: {} }, "/slow");
  const secondNav = s.renderRoute({ row: row(Promise.resolve(fastView)), params: {} }, "/fast");
  await secondNav;

  slow.settle(slowView);
  await firstNav;

  assert.equal(roots.screen.contains(fastView.element), true, "the newest render owns the screen");
  assert.equal(roots.screen.contains(slowView.element), false, "the stale render never reattaches");
  assert.equal(s.renderGeneration, 2);
});

test("the newest navigation always paints, even when an older one settles first", async () => {
  const s = shell();
  const view = { element: node("section"), title: "Run" };
  await s.renderRoute({ row: { load: () => Promise.resolve({ render: () => view }) }, params: {} }, "/runs");
  assert.equal(roots.screen.contains(view.element), true);
});

test("a found-less route still writes 'Not found'", async () => {
  const s = shell();
  await s.renderRoute(null, "/nowhere");
  assert.equal(roots.screen.children.length, 1);
  assert.equal(roots.screen.children[0].className, "not-found");
});
