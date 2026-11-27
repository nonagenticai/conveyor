import assert from "node:assert/strict";
import test from "node:test";

import { ProblemError, problemLine, problemText, readProblem } from "./problem.js";

test("maps an RFC 9457 problem with field errors", async () => {
  const response = new Response(
    JSON.stringify({
      type: "about:blank",
      title: "Unprocessable Entity",
      status: 422,
      detail: "name is required",
      code: "invalid_input",
      request_id: "req-1",
      errors: [{ field: "name", detail: "must not be empty" }],
    }),
    { status: 422, headers: { "content-type": "application/problem+json" } },
  );
  const problem = await readProblem(response);
  assert.equal(problem.status, 422);
  assert.equal(problem.code, "invalid_input");
  assert.equal(problem.detail, "name is required");
  assert.equal(problem.requestId, "req-1");
  assert.deepEqual(problem.fields, [{ field: "name", detail: "must not be empty" }]);
  const error = new ProblemError(problem);
  assert.equal(error.message, "name is required");
  assert.match(problemLine(problem), /invalid_input/);
});

test("a non-JSON body still yields a usable problem", async () => {
  const response = new Response("upstream said no", { status: 502, statusText: "Bad Gateway" });
  const problem = await readProblem(response);
  assert.equal(problem.status, 502);
  assert.equal(problem.title, "Bad Gateway");
  assert.equal(problem.detail, "");
});

test("problemText appends the field errors", () => {
  const problem = {
    status: 422,
    title: "Unprocessable Entity",
    code: "action-invalid-input",
    detail: "the input does not match the action schema",
    requestId: "req-2",
    fields: [
      { field: "action", detail: "has no approval to waive" },
      { field: "approval", detail: "must be `required` or `waived`" },
    ],
  };
  assert.equal(
    problemText(problem),
    "action-invalid-input — the input does not match the action schema — request req-2 · action: has no approval to waive · approval: must be `required` or `waived`",
  );
});

test("problemText without fields is the problem line", () => {
  const problem = {
    status: 409,
    title: "Conflict",
    code: "conflict",
    detail: "already promoted",
    requestId: "",
    fields: [],
  };
  assert.equal(problemText(problem), problemLine(problem));
});
