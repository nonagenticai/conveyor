// The action contract: the manifest is the authority, `actions.describe` the form source, and
// every write is `POST /api/actions/{name}/invocations` with the standard envelope and headers.
// No optimistic mutation: a caller refetches its read model after a write (SPEC §5).

import { ProblemError, readProblem } from "./problem.js";

/**
 * @typedef {{name: string, version?: number, kind?: string, spec_hash?: string,
 *            scopes?: string[], idempotent?: boolean, timeout_ms?: number}} ManifestEntry
 */

export class Api {
  /** @param {import("../auth/index.js").Auth} auth */
  constructor(auth) {
    this.auth = auth;
  }

  /**
   * @param {string} path
   * @param {{method?: string, body?: unknown, headers?: Record<string,string>,
   *          signal?: AbortSignal}} [options]
   * @returns {Promise<{value: any, response: Response}>}
   */
  async requestRaw(path, options = {}) {
    /** @type {Record<string, string>} */
    const headers = { ...this.auth.headers(), ...(options.headers ?? {}) };
    /** @type {RequestInit} */
    const init = {
      method: options.method ?? "GET",
      headers,
      credentials: "same-origin",
      cache: "no-store",
      signal: options.signal,
    };
    if (options.body !== undefined) {
      headers["content-type"] = "application/json";
      init.body = JSON.stringify(options.body);
    }
    const response = await fetch(path, init);
    if (!response.ok) {
      throw new ProblemError(await readProblem(response));
    }
    if (response.status === 204) {
      return { value: null, response };
    }
    const text = await response.text();
    const value = text.trim() === "" ? null : JSON.parse(text);
    return { value, response };
  }

  /**
   * @param {string} path
   * @param {{method?: string, body?: unknown, headers?: Record<string,string>,
   *          signal?: AbortSignal}} [options]
   */
  async request(path, options = {}) {
    return (await this.requestRaw(path, options)).value;
  }

  /** `GET /api/actions` — the role-filtered manifest. */
  async manifest() {
    const value = await this.request("/api/actions");
    return Array.isArray(value?.actions) ? value.actions : [];
  }

  /**
   * `actions.describe {name}` — input/output schema, effect, confirm, tenancy, spec hash.
   * @param {string} name
   */
  async describe(name) {
    const result = await this.invoke("actions.describe", { name });
    return result.output;
  }

  /**
   * `POST /api/actions/{name}/invocations`.
   * @param {string} name
   * @param {unknown} input
   * @param {{ifMatch?: number|string, idempotencyKey?: string, approvalId?: string,
   *          signal?: AbortSignal}} [options]
   * @returns {Promise<{output: any, etag: string|null}>}
   */
  async invoke(name, input, options = {}) {
    /** @type {Record<string, string>} */
    const headers = {};
    if (options.ifMatch !== undefined) headers["if-match"] = String(options.ifMatch);
    if (options.idempotencyKey) headers["idempotency-key"] = options.idempotencyKey;
    const body =
      options.approvalId === undefined
        ? { input }
        : { input, approval_id: options.approvalId };
    const { value, response } = await this.requestRaw(
      `/api/actions/${encodeURIComponent(name)}/invocations`,
      { method: "POST", body, headers, signal: options.signal },
    );
    return {
      output: value && typeof value === "object" && "output" in value ? value.output : value,
      etag: response.headers.get("etag"),
    };
  }
}
