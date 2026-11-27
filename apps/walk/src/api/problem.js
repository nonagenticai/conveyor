// RFC 9457 problem mapping (SPEC §5). The server's refusal is shown verbatim; the client never
// replaces it with a rule of its own.

/**
 * @typedef {{status: number, title: string, code: string, detail: string, requestId: string,
 *            fields: Array<{field: string, detail: string}>}} Problem
 */

/** @extends Error */
export class ProblemError extends Error {
  /** @param {Problem} problem */
  constructor(problem) {
    super(problem.detail || problem.title || `request refused (${problem.status})`);
    this.name = "ProblemError";
    /** @type {Problem} */
    this.problem = problem;
  }
}

/**
 * @param {Response} response
 * @returns {Promise<Problem>}
 */
export async function readProblem(response) {
  let body = null;
  try {
    body = await response.json();
  } catch {
    body = null;
  }
  const fields = [];
  const rawFields = body?.errors ?? body?.invalid_params ?? body?.field_errors;
  if (Array.isArray(rawFields)) {
    for (const item of rawFields) {
      if (item && typeof item === "object") {
        fields.push({
          field: String(item.field ?? item.name ?? item.pointer ?? ""),
          detail: String(item.detail ?? item.message ?? item.reason ?? ""),
        });
      }
    }
  }
  return {
    status: response.status,
    title: String(body?.title ?? response.statusText ?? ""),
    code: String(body?.code ?? body?.type ?? ""),
    detail: String(body?.detail ?? body?.message ?? ""),
    requestId: String(body?.request_id ?? response.headers.get("x-request-id") ?? ""),
    fields,
  };
}

/**
 * The human line for a problem: action code, server detail, request id.
 * @param {Problem} problem
 */
export function problemLine(problem) {
  const parts = [];
  if (problem.code) parts.push(problem.code);
  if (problem.detail) parts.push(problem.detail);
  else if (problem.title) parts.push(problem.title);
  if (problem.requestId) parts.push(`request ${problem.requestId}`);
  return parts.join(" — ");
}

/**
 * The full server line: the problem line, then each field error as `field: detail` — the same
 * shape `apps/web` renders for an invalid input (`api.rs` `Invalid`). A bare problem is
 * unchanged from `problemLine`.
 * @param {Problem} problem
 */
export function problemText(problem) {
  const parts = [problemLine(problem)];
  for (const item of problem.fields) parts.push(`${item.field}: ${item.detail}`);
  return parts.filter((part) => part !== "").join(" · ");
}
