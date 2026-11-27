// The canvas dispatch: a picked sprite must open the right sheet. `spriteAt` (frame.js) is
// unit-tested on its own; this covers the wiring between the pointer event, the hit and the
// wall/room callbacks, and the fallback panels when the engine is absent.

import assert from "node:assert/strict";
import test from "node:test";

import { World } from "./index.js";

const camera = { mode: "Third", position: [0, 0, 0], target: [0, 0, 0] };
const viewport = { width: 800, height: 600, scale: 40 };

function world() {
  const hits = { walls: [], rooms: [] };
  const canvas = {
    getContext: () => null,
    dataset: {},
    getBoundingClientRect: () => ({ left: 0, top: 0 }),
    addEventListener() {},
  };
  const worldData = {
    rooms: [{ id: "portfolio", label: "Portfolio & plan" }],
    walls: [{ walk: "/portfolio", room: "portfolio", view: "Portfolio & plan" }],
  };
  const w = new World({
    canvas,
    worldData,
    onWall: (wall) => hits.walls.push(wall),
    onRoom: (id) => hits.rooms.push(id),
  });
  w.viewport = () => viewport;
  w.engine = {}; // the engine branch is the one under test
  return { w, hits, worldData };
}

const wallView = {
  camera,
  overlays: [],
  sprites: [
    {
      id: { Wall: 0 },
      kind: "wall",
      position: [1, 0, 0],
      size: [1, 1],
      facing: 0,
      tint: "",
      label: "Wall",
    },
  ],
};

test("the engine config carries the seed and the census rows", () => {
  // The defect #839: `WorldConfig` has a required `seed`, and the app's config omitted it, so
  // the wasm constructor threw and every environment silently rendered the fallback.
  const { w } = world();
  const config = w.config();
  assert.equal(config.seed, 7, "the seed must be in the config the boundary parses");
  assert.deepEqual(config.rooms, [{ id: "portfolio", label: "Portfolio & plan" }]);
  assert.deepEqual(config.walls, [
    { route: "/portfolio", room: "portfolio", label: "Portfolio & plan", view: "Portfolio & plan" },
  ]);
});

test("a wall picked on the canvas opens that wall's sheet, not a room", () => {
  const { w, hits, worldData } = world();
  w.view = wallView;
  w.pointer({ clientX: 440, clientY: 360 });
  assert.deepEqual(hits.walls, [worldData.walls[0]]);
  assert.deepEqual(hits.rooms, []);
});

test("a click that hits no sprite does nothing", () => {
  const { w, hits } = world();
  w.view = wallView;
  w.pointer({ clientX: 10, clientY: 10 });
  assert.deepEqual(hits.walls, []);
  assert.deepEqual(hits.rooms, []);
});

test("a room picked on the canvas opens its room sheet", () => {
  const { w, hits } = world();
  w.view = {
    camera,
    overlays: [],
    sprites: [
      {
        id: { Room: 0 },
        kind: "room",
        position: [0, 0, 0],
        size: [2, 2],
        facing: 0,
        tint: "",
        label: "Room",
      },
    ],
  };
  w.pointer({ clientX: 400, clientY: 372 });
  assert.deepEqual(hits.rooms, ["portfolio"]);
  assert.deepEqual(hits.walls, []);
});

test("the fallback panels stay clickable without the engine", () => {
  const { w, hits } = world();
  w.engine = null;
  w.panels = [{ left: 0, top: 0, right: 100, bottom: 100, roomId: "portfolio" }];
  w.pointer({ clientX: 50, clientY: 50 });
  assert.deepEqual(hits.rooms, ["portfolio"]);
  w.pointer({ clientX: 500, clientY: 500 });
  assert.deepEqual(hits.rooms, ["portfolio"], "a click outside every panel does nothing");
});
