import assert from "node:assert/strict";
import test from "node:test";

import { fieldsFromSchema, humanise } from "./schema.js";

test("humanise turns a field name into a label", () => {
  assert.equal(humanise("owner_user_id"), "Owner user id");
  assert.equal(humanise("programmeId"), "Programme Id");
});

test("required, enum, number and JSON fields are typed", () => {
  const fields = fieldsFromSchema({
    type: "object",
    required: ["name"],
    properties: {
      name: { type: "string", title: "Name" },
      status: { type: "string", enum: ["active", "paused"] },
      version: { type: "integer", minimum: 1 },
      enabled: { type: "boolean" },
      payload: { type: "object" },
    },
  });
  assert.deepEqual(
    fields.map((field) => [field.name, field.kind, field.required]),
    [
      ["name", "string", true],
      ["status", "enum", false],
      ["version", "integer", false],
      ["enabled", "boolean", false],
      ["payload", "json", false],
    ],
  );
  assert.equal(fields[0].label, "Name");
  assert.deepEqual(fields[1].options, ["active", "paused"]);
  assert.equal(fields[2].minimum, 1);
});

test("an optional field's null union stays its value type, not JSON", () => {
  const fields = fieldsFromSchema({
    type: "object",
    properties: {
      name: { type: ["string", "null"] },
      owner: { anyOf: [{ type: "string", format: "uuid" }, { type: "null" }] },
      count: { type: ["integer", "null"], minimum: 1 },
    },
  });
  assert.deepEqual(
    fields.map((field) => field.kind),
    ["string", "string", "integer"],
  );
  assert.equal(fields[2].minimum, 1);
});

test("a schema without object properties yields no fields", () => {
  assert.deepEqual(fieldsFromSchema({ type: "string" }), []);
  assert.deepEqual(fieldsFromSchema(null), []);
});
