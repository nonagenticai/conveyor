/* console.js — the same-origin operator client for the design pack.
   ---------------------------------------------------------------------
   The public pack (nonagentic.ai/design) is a published snapshot and stays read-only. Served
   by the app itself (conveyor.nonagentic.ai/design) the pack is the operator console: this
   module signs in against the realm the server already trusts (auth-code + PKCE S256, the
   access token held in memory, ADR 0001) and calls the API the server already serves.

   Contract:
     * No config.json on this origin (the homepage mirror, file://) -> read-only, no request.
     * Signed out -> no API request and no write control; the pages keep the snapshot.
     * Signed in -> `GET /api/me`, the `/api/actions` manifest, and the literal read routes;
       every write is `POST /api/actions/<name>/invocations` with `{"input": …}`.
     * RFC 9457 problem documents are surfaced as they are; 401 clears the token, 412 carries
       the current representation so the caller refetches instead of guessing.

   `console_dev_token` is honoured only on a loopback host, so a stub server can drive the
   gate in tests; the real server's config.json never carries it.
*/
"use strict";
window.ConveyorConsole = (function () {
  const CONFIG_PATH = "/config.json";
  const ISSUES_PATH = "data/issues.json";
  const VERIFIER_KEY = "conveyor.console.verifier";
  const STATE_KEY = "conveyor.console.state";
  const RETURN_KEY = "conveyor.console.return";
  const DEFAULT_TENANT = "nonagentic";
  const PUBLIC_CONSOLE = "https://conveyor.nonagentic.ai/design/";

  class ConsoleError extends Error {
    constructor(status, problem, path) {
      const detail =
        (problem && (problem.detail || problem.title)) || `HTTP ${status}${path ? " on " + path : ""}`;
      super(detail);
      this.name = "ConsoleError";
      this.status = status;
      this.title = (problem && problem.title) || "Request failed";
      this.detail = detail;
      this.errors = (problem && problem.errors) || [];
      this.problem = problem || null;
      this.path = path || "";
    }
  }

  const listeners = new Set();
  const state = {
    config: null,
    token: null,
    me: null,
    tenant: null,
    manifest: null,
    manifestError: null,
    error: null,
    ready: false,
  };

  /* ------------------------------------------------------------------ helpers */
  const http = () => location.protocol === "http:" || location.protocol === "https:";
  const loopback = () =>
    ["127.0.0.1", "localhost", "::1", "[::1]"].includes(location.hostname);

  const store = (key, value) => {
    try {
      sessionStorage.setItem(key, value);
    } catch (_) {
      /* a private window may refuse; the flow then simply restarts */
    }
  };
  const unstore = (key) => {
    try {
      sessionStorage.removeItem(key);
    } catch (_) {
      /* ignore */
    }
  };
  const stored = (key) => {
    try {
      return sessionStorage.getItem(key) || "";
    } catch (_) {
      return "";
    }
  };

  const b64url = (bytes) =>
    btoa(String.fromCharCode.apply(null, Array.from(bytes)))
      .replace(/\+/g, "-")
      .replace(/\//g, "_")
      .replace(/=+$/, "");

  const randomB64url = (size = 32) => b64url(crypto.getRandomValues(new Uint8Array(size)));

  const sha256b64url = async (text) => {
    const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
    return b64url(new Uint8Array(digest));
  };

  const redirectUri = () => location.origin + location.pathname;
  const tokenEndpoint = () => (state.config ? state.config.issuer.replace(/\/$/, "") : "");

  const problemOf = async (response) => {
    try {
      return await response.json();
    } catch (_) {
      return null;
    }
  };

  const emit = () => {
    const status = statusOf();
    listeners.forEach((fn) => {
      try {
        fn(status);
      } catch (_) {
        /* a listener's failure is its own; it must not break the page */
      }
    });
    chip(); /* the chip reflects a session a request just ended (401) as well as a click */
    try {
      window.dispatchEvent(new CustomEvent("conveyor-console", { detail: status }));
    } catch (_) {
      /* no DOM (tests) */
    }
  };

  const statusOf = () => ({
    ready: state.ready,
    signedIn: !!state.token,
    name:
      (state.me && state.me.principal && state.me.principal.display_name) ||
      (state.config && state.config.client_id) ||
      "",
    tenant: state.tenant || "",
    issuer: (state.config && state.config.issuer) || "",
    error: state.error ? String(state.error.message || state.error) : null,
    manifestError: state.manifestError
      ? String(state.manifestError.message || state.manifestError)
      : null,
  });

  /* ------------------------------------------------------------------ requests */
  const headersFor = (extra = {}) => {
    const headers = Object.assign({ Accept: "application/json" }, extra);
    if (state.token) headers.Authorization = "Bearer " + state.token;
    if (state.tenant) headers["x-active-tenant"] = state.tenant;
    return headers;
  };

  const handle = async (response, path) => {
    if (response.ok) return response;
    const problem = await problemOf(response);
    if (response.status === 401) {
      state.token = null;
      state.me = null;
      state.manifest = null;
      emit();
    }
    throw new ConsoleError(response.status, problem, path);
  };

  async function get(path) {
    if (!state.token) throw new ConsoleError(401, { title: "Not signed in", detail: "Sign in to read the platform's rows." }, path);
    const response = await fetch(path, {
      method: "GET",
      headers: headersFor(),
      credentials: "same-origin",
      cache: "no-store",
    });
    return (await handle(response, path)).json();
  }

  async function action(name, input, options = {}) {
    if (!state.token) throw new ConsoleError(401, { title: "Not signed in", detail: "Sign in to operate the programme." }, name);
    const extra = { "Content-Type": "application/json" };
    if (options.etag != null) extra["If-Match"] = `"${options.etag}"`;
    const path = `/api/actions/${name}/invocations`;
    const response = await fetch(path, {
      method: "POST",
      headers: headersFor(extra),
      credentials: "same-origin",
      body: JSON.stringify({ input: input == null ? {} : input }),
    });
    return (await handle(response, path)).json();
  }

  async function issues() {
    const response = await fetch(ISSUES_PATH, { cache: "no-store" });
    if (!response.ok) {
      throw new ConsoleError(response.status, { title: "Issue registry unavailable", detail: `GET ${ISSUES_PATH} answered ${response.status}` }, ISSUES_PATH);
    }
    const data = await response.json();
    return Array.isArray(data.issues) ? data.issues : [];
  }

  /* ------------------------------------------------------------------ sign-in */
  function signIn() {
    if (!state.config || !state.config.issuer || !state.config.client_id) {
      state.error = new Error("This origin has no sign-in configuration (/config.json).");
      emit();
      return;
    }
    const verifier = randomB64url(48);
    const nonce = randomB64url(16);
    store(VERIFIER_KEY, verifier);
    store(STATE_KEY, nonce);
    store(RETURN_KEY, location.hash || "");
    sha256b64url(verifier)
      .then((challenge) => {
        const query = new URLSearchParams({
          client_id: state.config.client_id,
          response_type: "code",
          scope: "openid profile email",
          redirect_uri: redirectUri(),
          code_challenge: challenge,
          code_challenge_method: "S256",
          state: nonce,
        });
        location.assign(`${tokenEndpoint()}/protocol/openid-connect/auth?${query}`);
      })
      .catch((error) => {
        state.error = error;
        emit();
      });
  }

  async function exchange(code) {
    const verifier = stored(VERIFIER_KEY);
    if (!verifier) throw new Error("the PKCE verifier is gone; sign in again");
    const body = new URLSearchParams({
      grant_type: "authorization_code",
      client_id: state.config.client_id,
      code,
      redirect_uri: redirectUri(),
      code_verifier: verifier,
    });
    const response = await fetch(`${tokenEndpoint()}/protocol/openid-connect/token`, {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded", Accept: "application/json" },
      body,
    });
    if (!response.ok) throw new ConsoleError(response.status, await problemOf(response), "token");
    const token = await response.json();
    state.token = token.access_token || null;
    unstore(VERIFIER_KEY);
    unstore(STATE_KEY);
    if (!state.token) throw new Error("the token response carried no access_token");
  }

  async function callback() {
    const params = new URLSearchParams(location.search);
    const code = params.get("code");
    const returned = params.get("state");
    const expected = stored(STATE_KEY);
    if (!code) return false;
    if (!expected || returned !== expected) {
      unstore(VERIFIER_KEY);
      unstore(STATE_KEY);
      throw new Error("the sign-in response did not match this browser's state");
    }
    await exchange(code);
    const back = stored(RETURN_KEY);
    unstore(RETURN_KEY);
    const clean = location.pathname + (back || "");
    history.replaceState(null, "", clean);
    return true;
  }

  function signOut() {
    state.token = null;
    state.me = null;
    state.tenant = null;
    state.manifest = null;
    state.manifestError = null;
    emit();
  }

  async function loadMe() {
    const me = await get("/api/me");
    state.me = me;
    const memberships = Array.isArray(me.memberships) ? me.memberships : [];
    const preferred = (state.config && state.config.console_tenant) || DEFAULT_TENANT;
    const chosen =
      memberships.find((row) => row.slug === preferred) ||
      memberships[0] ||
      null;
    state.tenant = me.active_tenant ? me.active_tenant.slug : chosen ? chosen.slug : null;
    return me;
  }

  async function loadManifest() {
    try {
      const document = await get("/api/actions");
      const entries = Array.isArray(document.actions) ? document.actions : [];
      state.manifest = new Set(entries.map((row) => row.name));
      state.manifestError = null;
    } catch (error) {
      state.manifest = null;
      state.manifestError = error;
    }
  }

  /* ------------------------------------------------------------------ the chip */
  function chip() {
    const actions = document.querySelector(".top-actions");
    if (!actions || !state.config || (!state.config.issuer && !state.config.console_dev_token)) return;
    let button = document.getElementById("console-chip");
    if (!button) {
      button = document.createElement("button");
      button.type = "button";
      button.id = "console-chip";
      button.className = "console-chip";
      const search = actions.querySelector(".search-button");
      actions.insertBefore(button, search || actions.firstChild);
      button.addEventListener("click", () => {
        if (state.token) signOut();
        else signIn();
      });
    }
    const signed = !!state.token;
    button.dataset.state = signed ? "in" : "out";
    button.title = signed
      ? `Signed in as ${statusOf().name}${state.tenant ? " · " + state.tenant : ""} — click to sign out`
      : "Sign in to operate the programme";
    button.innerHTML = signed
      ? `<i></i><span>${escapeHtml(statusOf().name || "Operator")}</span><small>${escapeHtml(state.tenant || "")}</small>`
      : `<i></i><span>Sign in</span><small>operator</small>`;
  }

  function escapeHtml(value) {
    return String(value == null ? "" : value).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
  }

  function paint() {
    chip();
    emit();
  }

  /* ------------------------------------------------------------------ boot */
  async function boot() {
    if (!http()) {
      state.ready = true;
      return; /* file:// — DESIGN mode, zero requests */
    }
    try {
      const response = await fetch(CONFIG_PATH, {
        headers: { Accept: "application/json" },
        cache: "no-store",
      });
      if (response.ok) state.config = await response.json();
    } catch (_) {
      state.config = null; /* the homepage mirror answers HTML 404 */
    }
    try {
      if (state.config && state.config.console_dev_token && loopback()) {
        state.token = String(state.config.console_dev_token);
      }
      if (!state.token && state.config && state.config.issuer) await callback();
      if (state.token) {
        await loadMe();
        await loadManifest();
      }
    } catch (error) {
      state.error = error;
      state.token = null;
    }
    state.ready = true;
    if (document.readyState === "loading") {
      document.addEventListener("DOMContentLoaded", paint, { once: true });
    } else {
      paint();
    }
    emit();
  }

  if (typeof document !== "undefined") {
    if (document.readyState === "loading") {
      document.addEventListener("DOMContentLoaded", () => chip(), { once: true });
    } else {
      chip();
    }
  }
  boot();

  return {
    /** The same-origin config, or null (homepage mirror / file://). */
    config: () => state.config,
    /** True once a bearer is held in memory. */
    signedIn: () => !!state.token,
    /** The principal and tenant, or null. */
    me: () => state.me,
    tenant: () => state.tenant,
    /** A Set of the action names this caller may invoke, or null. */
    manifest: () => state.manifest,
    can: (name) => !!state.token && !!state.manifest && state.manifest.has(name),
    status: statusOf,
    error: () => state.error,
    onChange: (fn) => {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    notify: () => emit(),
    signIn,
    signOut,
    get,
    action,
    issues,
    /** Where an operator signs in; the public site links here. */
    consoleUrl: () => PUBLIC_CONSOLE,
    ConsoleError,
  };
})();
