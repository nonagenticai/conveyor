import assert from "node:assert/strict";
import test from "node:test";

import { base64url, challengeS256, randomString } from "./pkce.js";

// RFC 7636 appendix B: the verifier and the S256 challenge it must produce.
test("S256 matches the RFC 7636 vector", async () => {
  const verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
  assert.equal(
    await challengeS256(verifier),
    "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
  );
});

test("base64url has no padding and no URL-unsafe characters", () => {
  const encoded = base64url(new Uint8Array([251, 255, 254, 0, 1, 2]));
  assert.doesNotMatch(encoded, /[+/=]/);
});

test("randomString is URL-safe and distinct", () => {
  const first = randomString(32);
  const second = randomString(32);
  assert.notEqual(first, second);
  assert.match(first, /^[A-Za-z0-9_-]+$/);
  assert.ok(first.length >= 32);
});
