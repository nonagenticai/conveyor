// The two identity paths the walk must carry: the dev cookie/memory token for local runs and
// auth-code + PKCE S256 for production. The spike proves the URL/verifier mechanics; the real
// issuer redirect is the external prerequisite the record names.
export async function loadConfig() {
  try {
    const response = await fetch("/config.json", { credentials: "same-origin" });
    if (!response.ok) return null;
    return await response.json();
  } catch {
    return null;
  }
}

export async function devLogin(login = "alice") {
  await fetch(`/__dev/login?login=${encodeURIComponent(login)}`, {
    method: "POST",
    credentials: "same-origin",
  });
}

function randomToken(bytes) {
  const buffer = new Uint8Array(bytes);
  crypto.getRandomValues(buffer);
  return base64url(buffer);
}

function base64url(bytes) {
  let text = "";
  for (const byte of bytes) text += String.fromCharCode(byte);
  return btoa(text).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

export async function pkce(config, redirectUri) {
  const verifier = randomToken(32);
  const state = randomToken(16);
  const digest = new Uint8Array(
    await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier)),
  );
  const challenge = base64url(digest);
  sessionStorage.setItem("spike.pkce.verifier", verifier);
  sessionStorage.setItem("spike.pkce.state", state);

  const issuer = (config?.issuer ?? "").trim().replace(/\/$/, "");
  const clientId = (config?.client_id ?? "").trim();
  const redirect = redirectUri ?? `${location.origin}/walk/`;
  if (!issuer || !clientId) {
    return {
      ok: false,
      reason: "no issuer/client_id in /config.json — the OIDC redirect is the external prerequisite",
      issuer,
      clientId,
      redirectUri: redirect,
      verifier,
      challenge,
      state,
    };
  }
  const url = new URL(`${issuer}/protocol/openid-connect/auth`);
  url.search = new URLSearchParams({
    client_id: clientId,
    redirect_uri: redirect,
    response_type: "code",
    scope: "openid email profile",
    code_challenge: challenge,
    code_challenge_method: "S256",
    state,
  }).toString();
  return {
    ok: true,
    url: url.toString(),
    issuer,
    clientId,
    redirectUri: redirect,
    verifier,
    challenge,
    state,
  };
}

export function begin(config) {
  return pkce(config).then((result) => {
    if (result.ok) location.assign(result.url);
    return result;
  });
}
