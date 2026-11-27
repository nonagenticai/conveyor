import assert from "node:assert/strict";
import test from "node:test";

import { fitScale, project, spriteAt, spriteRect } from "./frame.js";

const camera = { mode: "Third", position: [0, 0, 0], target: [0, 0, 0] };
const viewport = { width: 800, height: 600, scale: 40 };

test("the camera centre projects to the middle of the canvas", () => {
  const point = project([0, 0, 0], camera, viewport);
  assert.equal(point.x, 400);
  assert.equal(point.y, 372);
});

test("a sprite rect sits above its floor point", () => {
  const sprite = {
    id: { Wall: 0 },
    kind: "wall",
    position: [1, 0, 0],
    size: [1, 1],
    facing: 0,
    tint: "#2f8070",
    label: "Wall",
  };
  const rect = spriteRect(sprite, camera, viewport);
  assert.equal(rect.centre.x, 440);
  assert.equal(rect.top, 332);
  assert.equal(rect.bottom, 372);
});

test("picking honours the array order and ignores id-less sprites", () => {
  const view = {
    camera,
    overlays: [],
    sprites: [
      { kind: "room", position: [1, 0, 0], size: [2, 2], facing: 0, tint: "", label: "Room" },
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
  assert.equal(spriteAt(view, 440, 360, viewport)?.kind, "wall");
  assert.equal(spriteAt(view, 10, 10, viewport), null);
});

test("the fit scale keeps a ring visible", () => {
  assert.ok(fitScale(1200, 800) >= 18);
  assert.ok(fitScale(1200, 800) <= 800 / 18);
});
