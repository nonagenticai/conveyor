import assert from "node:assert/strict";
import test from "node:test";

import { ProblemError, problemLine, readProblem } from "./problem.js";

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
