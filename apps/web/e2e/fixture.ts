import { readFileSync } from "node:fs";

import { test as base, expect } from "@playwright/test";

// #391: the deployed (OIDC) harness-owned bearer.
//
// The app keeps its access token in memory only (ADR 0001): a fresh document load cannot
// authenticate itself, and `EventSource` is not used — every API call, including the SSE stream,
// is a `fetch`. So the harness logs in once in the `oidc` setup project, writes the bearer, and
// injects it into every `/api/**` request at the network layer. The app's token policy is
// untouched: this is a test-only transport seam, installed only under OIDC.
//
// Dev mode (DEV_AUTH_TOKEN set) installs nothing: the `conveyor_dev` cookie is the credential.
//
// The signed-out case needs no opt-out: a test that builds its own context from `browser`
// (shell-login's "a browser without a credential") never instantiates this fixture.

const TOKEN_FILE = process.env.E2E_TOKEN_FILE ?? "test-results/oidc-token.txt";
const oidc = Boolean(process.env.BASE_URL) && !process.env.DEV_AUTH_TOKEN;

function bearer(): string {
  const token = readFileSync(TOKEN_FILE, "utf8").trim();
  if (!token) {
    throw new Error(`no OIDC token at ${TOKEN_FILE}; did the oidc setup project run?`);
  }
  return token;
}

export const test = base.extend({
  // The context fixture, not the page: every page a test opens in its own context gets the
  // header, including pages the app opens itself.
  context: async ({ context }, use) => {
    if (oidc) {
      const token = bearer();
      await context.route("**/api/**", (route) => {
        const headers = { ...route.request().headers(), authorization: `Bearer ${token}` };
        return route.continue({ headers });
      });
    }
    await use(context);
  },
});

export { expect };
export * from "@playwright/test";
