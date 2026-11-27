// PKCE S256 helpers and base64url. The verifier lives in sessionStorage for the round trip;
// the access token never leaves memory (SPEC §9).

/** @param {Uint8Array} bytes @returns {string} */
export function base64url(bytes) {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

/** @param {number} [bytes] @returns {string} */
export function randomString(bytes = 32) {
  const buffer = new Uint8Array(bytes);
  crypto.getRandomValues(buffer);
  return base64url(buffer);
}

/** @param {string} verifier @returns {Promise<string>} */
export async function challengeS256(verifier) {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier));
  return base64url(new Uint8Array(digest));
}

/**
 * The authorize URL. `redirect_uri` is the origin root: a signed-out deep link is restored
 * from sessionStorage after the callback, so it never has to be a redirect URI itself.
 * @param {{issuer: string, clientId: string}} config
 * @param {string} challenge
 * @param {string} state
 * @returns {string}
 */
export function authorizeUrl(config, challenge, state) {
  const base = config.issuer.replace(/\/+$/, "");
  const url = new URL(`${base}/protocol/openid-connect/auth`);
  url.searchParams.set("response_type", "code");
  url.searchParams.set("client_id", config.clientId);
  url.searchParams.set("redirect_uri", redirectUri());
  url.searchParams.set("scope", "openid email profile");
  url.searchParams.set("state", state);
  url.searchParams.set("code_challenge", challenge);
  url.searchParams.set("code_challenge_method", "S256");
  return url.toString();
}

/** @returns {string} */
export function redirectUri() {
  return `${window.location.origin}/`;
}
