// Identity: auth-code + PKCE S256, the access token in memory only, the dev cookie for local
// and CI runs (SPEC §9, apps/web/src/auth.rs). `/config.json` decides which public client the
// origin uses; the server enforces the azp allowlist (ADR 0024).

import { modes } from "./config.js";
import { authorizeUrl, challengeS256, randomString, redirectUri } from "./pkce.js";

const VERIFIER_KEY = "conveyor.walk.pkce.verifier";
const STATE_KEY = "conveyor.walk.pkce.state";
const RETURN_KEY = "conveyor.walk.return";
const TENANT_KEY = "conveyor.walk.tenant";

/** @typedef {{tenant_id: string, slug: string, name: string, role: string}} Membership */
/** @typedef {{principal: {id: string, subject: string, display_name: string, email?: string|null}, active_tenant: Membership|null, memberships: Membership[], is_super_admin?: boolean}} Me */

export class Auth {
  /** @param {import("./config.js").WalkConfig} config */
  constructor(config) {
    this.config = config;
    /** @type {string|null} */
    this.token = null;
    /** @type {Me|null} */
    this.me = null;
    /** @type {string|null} */
    this.tenant = null;
    /** @type {string|null} */
    this.error = null;
  }

  modes() {
    return modes(this.config);
  }

  /** Complete a callback if present, then resolve the current session. */
  async boot() {
    const url = new URL(window.location.href);
    if (url.searchParams.has("code") || url.searchParams.has("error")) {
      await this.completeCallback(url);
    }
    this.tenant = sessionStorage.getItem(TENANT_KEY);
    await this.refreshMe();
    // One membership needs no choice: name it so every later call carries the tenant.
    if (this.me && !this.me.active_tenant && this.me.memberships.length === 1) {
      this.tenant = this.me.memberships[0].tenant_id;
      sessionStorage.setItem(TENANT_KEY, this.tenant);
      await this.refreshMe();
    }
    return this.me;
  }

  /** @returns {Promise<Me|null>} */
  async refreshMe() {
    const response = await fetch("/api/me", {
      cache: "no-store",
      credentials: "same-origin",
      headers: this.headers(),
    });
    if (response.status === 401) {
      this.me = null;
      return null;
    }
    if (!response.ok) {
      this.me = null;
      this.error = `me: ${response.status}`;
      return null;
    }
    this.me = await response.json();
    return this.me;
  }

  /** @returns {Record<string, string>} */
  headers() {
    const headers = { accept: "application/json" };
    if (this.token) headers.authorization = `Bearer ${this.token}`;
    if (this.tenant) headers["x-active-tenant"] = this.tenant;
    return headers;
  }

  signedIn() {
    return this.me !== null;
  }

  principal() {
    return this.me ? this.me.principal : null;
  }

  memberships() {
    return this.me ? this.me.memberships : [];
  }

  /** @param {string|null} tenantId */
  async selectTenant(tenantId) {
    this.tenant = tenantId;
    if (tenantId) sessionStorage.setItem(TENANT_KEY, tenantId);
    else sessionStorage.removeItem(TENANT_KEY);
    return this.refreshMe();
  }

  /**
   * Start the auth-code + PKCE round trip. The signed-out path is kept so the callback can
   * return the user to the deep link they opened.
   * @param {string} [returnPath]
   */
  async signIn(returnPath = window.location.pathname + window.location.search) {
    sessionStorage.setItem(RETURN_KEY, returnPath);
    const verifier = randomString(64);
    const challenge = await challengeS256(verifier);
    const state = randomString(32);
    sessionStorage.setItem(VERIFIER_KEY, verifier);
    sessionStorage.setItem(STATE_KEY, state);
    window.location.assign(authorizeUrl(this.config, challenge, state));
  }

  /**
   * The dev cookie: POST /__dev/login sets it, then the document reloads into a boot that
   * succeeds. Only offered when `/config.json` says dev_auth is on.
   * @param {string} [login]
   * @param {string} [returnPath]
   */
  async devLogin(login = "alice", returnPath = window.location.pathname + window.location.search) {
    const response = await fetch(`/__dev/login?login=${encodeURIComponent(login)}`, {
      method: "POST",
      credentials: "same-origin",
      redirect: "follow",
    });
    if (!response.ok) throw new Error(`dev login answered ${response.status}`);
    window.location.assign(returnPath || "/");
  }

  /** Clear the in-memory session and the dev cookie. The IdP session is not touched. */
  async signOut() {
    this.token = null;
    this.me = null;
    this.tenant = null;
    for (const key of [VERIFIER_KEY, STATE_KEY, RETURN_KEY, TENANT_KEY]) {
      sessionStorage.removeItem(key);
    }
    document.cookie = "conveyor_dev=; Max-Age=0; Path=/";
    window.location.assign("/");
  }

  /** @param {URL} url */
  async completeCallback(url) {
    const state = url.searchParams.get("state");
    const code = url.searchParams.get("code");
    const error = url.searchParams.get("error");
    const expected = sessionStorage.getItem(STATE_KEY);
    const verifier = sessionStorage.getItem(VERIFIER_KEY);
    const returnTo = sessionStorage.getItem(RETURN_KEY) || "/";
    sessionStorage.removeItem(STATE_KEY);
    sessionStorage.removeItem(VERIFIER_KEY);
    sessionStorage.removeItem(RETURN_KEY);
    // Remove the one-time code from the address bar before any other work.
    history.replaceState(null, "", returnTo);
    if (error) {
      this.error = `sign-in refused: ${error}`;
      return;
    }
    if (!code || !state || !expected || state !== expected || !verifier) {
      this.error = "sign-in state did not match";
      return;
    }
    const base = this.config.issuer.replace(/\/+$/, "");
    const body = new URLSearchParams({
      grant_type: "authorization_code",
      client_id: this.config.clientId,
      redirect_uri: redirectUri(),
      code,
      code_verifier: verifier,
    });
    const response = await fetch(`${base}/protocol/openid-connect/token`, {
      method: "POST",
      headers: { "content-type": "application/x-www-form-urlencoded", accept: "application/json" },
      body,
    });
    if (!response.ok) {
      this.error = `token exchange refused (${response.status})`;
      return;
    }
    const payload = await response.json();
    if (typeof payload.access_token !== "string" || payload.access_token === "") {
      this.error = "token exchange returned no access token";
      return;
    }
    this.token = payload.access_token;
  }
}
