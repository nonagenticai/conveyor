// The spike's API client: one call shape for reads and writes, and a trace the test asserts
// against. Dev transport only (`dev:<login>`); production uses the in-memory PKCE token.
export const DEV_TOKEN = "dev:alice";
export const trace = [];

let token = DEV_TOKEN;

export function setToken(value) {
  token = value;
}

export async function request(path, opts = {}) {
  const headers = { ...(opts.headers ?? {}) };
  if (token && !headers.authorization) headers.authorization = `Bearer ${token}`;
  if (opts.tenant) headers["x-active-tenant"] = opts.tenant;
  if (opts.ifMatch !== undefined && opts.ifMatch !== null) headers["if-match"] = String(opts.ifMatch);
  if (opts.idempotencyKey) headers["idempotency-key"] = opts.idempotencyKey;
  const init = { method: opts.method ?? "GET", headers, credentials: "same-origin" };
  if (opts.body !== undefined) {
    init.body = JSON.stringify(opts.body);
    if (!headers["content-type"]) headers["content-type"] = "application/json";
  }
  const entry = { path, method: init.method, headers: { ...headers }, body: opts.body ?? null };
  trace.push(entry);

  const response = await fetch(path, init);
  const text = await response.text();
  let body = null;
  try {
    body = text ? JSON.parse(text) : null;
  } catch {
    body = text;
  }
  if (!response.ok) {
    const error = new Error(body?.detail ?? body?.title ?? `HTTP ${response.status}`);
    error.status = response.status;
    error.problem = body;
    error.requestId = body?.request_id;
    throw error;
  }
  return body;
}

export async function invoke(name, input, opts = {}) {
  const body = opts.approvalId ? { input, approval_id: opts.approvalId } : { input };
  const document = await request(`/api/actions/${encodeURIComponent(name)}/invocations`, {
    ...opts,
    method: "POST",
    body,
  });
  return { output: document.output, document };
}

export async function describe(name) {
  return (await invoke("actions.describe", { name })).output;
}

export async function me() {
  return request("/api/me");
}
