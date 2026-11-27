import assert from "node:assert/strict";
import test from "node:test";

import { parseSseBuffer } from "./events.js";

test("parses complete frames and keeps the partial tail", () => {
  const buffer =
    "event: board\nid: 7\ndata: {\"a\":1}\n\n" +
    ": keepalive\n\n" +
    "event: card\ndata: one\ndata: two\n\n" +
    "event: partial\ndata: {";
  const { frames, rest } = parseSseBuffer(buffer);
  assert.equal(frames.length, 2);
  assert.deepEqual(frames[0], { id: "7", event: "board", data: "{\"a\":1}" });
  assert.deepEqual(frames[1], { id: "", event: "card", data: "one\ntwo" });
  assert.equal(rest, "event: partial\ndata: {");
});

test("an empty buffer is an empty result", () => {
  assert.deepEqual(parseSseBuffer(""), { frames: [], rest: "" });
});
