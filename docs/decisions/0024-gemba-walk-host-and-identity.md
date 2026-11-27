# 0024 — The Gemba Walk is served at `game.nonagentic.ai` with its own public client

**Status:** Accepted · 2026-11-27 · **Approved:** delegated rule decision under the published
delegation of 2026-11-22 (reference `01a1fb9f-ac2d-732d-b274-1079be3dd2b3`; expiry 2027-02-18) —
not a human signature · **Owner direction:** 2026-11-27 · **Change request:** CR-WALK-001
(`docs/plans/inception/GEMBA-WALK/08-change-request-001-host-and-identity.md`)
**Supersedes in part:** ADR 0021 decision 1 (the `/walk/` sub-site on the operator origin) and
decision 4's "same public client" default; SPEC_V2 D67 in part.

## Context

ADR 0021 authorised a separate game-style client but placed it at `/walk/` on the operator origin,
using the `conveyor-web` public client and deferring production sign-in to an external redirect-URI
registration. That was the conservative first step: one image, one origin, one client, no identity
work.

The owner has now directed (2026-11-27) that the game is a secondary UI that **may become the
primary UI**, that it must have **its own Keycloak setup** and `https://game.nonagentic.ai`, and
that the UI must work first time. A sub-path on the operator origin cannot be that product URL, and
a production sign-in that waits on an external registration is not a working identity.

The estate has the pieces to do this without a second runtime: `conveyor-public-vs` is an Istio
VirtualService behind the `cloudflared-apps` tunnel; the gateway already carries the
`*.nonagentic.ai` wildcard and the origin certificate is not verified; DNS is derived from the
tunnel ConfigMap by the `dns-ensure` CronJob; the conveyor realm `conveyor` already holds the
`conveyor-web` public client with PKCE S256 and a `conveyor` audience mapper; clients are managed by
a git-driven, idempotent realm-patch ConfigMap and a manual Job; and the realm already carries the
E2E principal `conveyor-e2e`. The API is a single axum service that already serves both the operator
SPA and a static tree; the walk is a static ESM tree plus an optional wasm module.

## Decision

1. **The canonical URL is `https://game.nonagentic.ai/`, served at the root.** The walk's paths
   are the `apps/web` paths (`/portfolio`, `/board/:board_id`), so parity is an equality, not a
   prefix mapping. The `/walk` prefix retires in production. `conveyor.nonagentic.ai/walk` and
   `/walk/*` return a 308 to the game host with the prefix stripped; the sub-site path remains only
   as the no-host dev/CI shape so the existing spike and local workflow keep a shape.
2. **One image, one service, two hosts.** `game.nonagentic.ai` is added to `conveyor-public-vs` and
   routes to the existing `conveyor` service. The API is same-origin on both hosts: no CORS, the
   same SSE transport, the same dev cookie in development. No second deployment, port, database or
   runtime.
3. **The game host is a hard boundary.** The server dispatches on the normalised `Host`; on a game
   host it serves only the walk tree, `/config.json` and the API, and it refuses the operator
   surfaces (`/mcp`, `/design`) and never serves the operator index or a directory index. The gate
   is a router-level middleware, because axum routes pre-empt the fallback and `/mcp` is a real
   route; a check inside the fallback alone would leak it. `/config.json` returns the game client id
   on the game host. The `/walk` redirect target is configuration (`CONVEYOR_GAME_PUBLIC_URL`),
   never the request `Host`, so the redirect cannot be pointed at an attacker's host.
4. **The game has its own public Keycloak client, `conveyor-game`, in the existing realm
   `conveyor`.** It is a public client with the standard flow, PKCE S256, redirect
   `https://game.nonagentic.ai/*`, web origin `https://game.nonagentic.ai` (the token endpoint's
   CORS), the same default/optional scopes, and the `conveyor` audience mapper. It is managed by the
   realm-patch ConfigMap with its own create-or-refuse convergence and drift preflight; the manual
   Job is run as part of the unit.
5. **A new realm is rejected.** A realm per surface would duplicate the user set, break single
   sign-on and create a second identity boundary to operate. The realm is already per app
   (`conveyor`); the game's isolation is a client, not a realm. If realm-level isolation is ever
   required, it is a separate change with a user-migration cost.
6. **The API accepts an `azp` allowlist of exactly `{conveyor-web, conveyor-game}`.** The configured
   audience remains `conveyor`. Both are first-party browser clients of the same principal set; the
   replay guard stays "one of our two clients", not "any realm client". The game client id is an
   explicit optional configuration value, never defaulted from the audience.
7. **Deployment order is part of the decision.** The conveyor image (host-aware, walk tree, config
   per host) is deployed first with the game host configured but **not routed**; then the gitops
   change routes the host and sets the env; then the realm Job runs; then the live proof. This
   guarantees the operator app is never served at the game host.
8. **The live proof is a real sign-in.** A deployed Playwright spec signs in as `conveyor-e2e`
   through the game client, captures the memory-only token from the first `/api` request, and
   asserts the Portfolio read and `/api/me`. It reuses the established `apps/web/e2e/oidc.setup.ts`
   pattern; credentials are env-only and never committed. A redirect that merely reaches Keycloak is
   not acceptance.
9. **The P6 primacy decision is unchanged.** `apps/web` remains normative until the measured parity
   gate and the owner's decision. This ADR makes the walk first-class; it does not promote it.

## Why

- The owner named the URL and the identity isolation. Both are product decisions the architecture
  must express, not defer.
- The same image and service keeps the blast radius and the operations surface unchanged; the
  second host is a routing and identity change, not a platform.
- Same-origin keeps the browser contract simple: no CORS layer, the same SSE implementation, the
  same cookie discipline in development, and no token in a cross-origin URL.
- A dedicated client gives the game its own redirect set, web origin and session policy without
  duplicating users; the `azp` allowlist keeps the API's replay guard meaningful.
- Root paths make the parity instrument an equality and remove a prefix that would otherwise have
  to be migrated at primacy.

## Alternatives considered

- **Keep the `/walk/` sub-site and add a redirect URI to `conveyor-web`.** Rejected: it does not
  give the game its own URL or client, and it leaves the production sign-in dependent on widening
  the operator client's redirect set — the opposite of isolation.
- **A new Keycloak realm for the game.** Rejected: user duplication, two sign-on boundaries, two
  sets of realm settings and secrets, for no capability. The estate's policy is a realm per app.
- **A second deployment or image for the walk.** Rejected: doubles the runtime, probes, secrets,
  image pipeline and CI for a static tree that is already served by the existing service. It would
  also need CORS or a second API origin.
- **A CDN or a new port.** Rejected: the tunnel and the Istio host already provide the edge; a new
  port is operational surface with no benefit. Port 8000 is never used.
- **Deriving the redirect target from the request `Host`.** Rejected: an open redirect. The target
  is configuration.

## Consequences

- `crates/auth` gains an optional game client id and a two-id `azp` allowlist; `crates/api` gains
  the game hosts, the game public URL, the router-level host gate and host-aware static/config
  serving. Tests cover the host matrix, the boundary and the redirect.
- `apps/walk` is served from `/srv/site/walk` on the game host; assets are relative so the same tree
  also works under the no-host sub-site. The wasm build runs in the image stage and CI, never on a
  developer machine.
- The gitops repo gains the client definition, the tunnel ingress, the VirtualService host and the
  deployment env; the realm Job is re-run and read back.
- The census's `walk` paths become host-rooted; the W0 record is re-baselined with a signed commit
  and the 52/598 counts recorded.
- ADR 0021's parity goal, the action registry, the data model, the no-fixture rule and the P6 gate
  are unchanged.
- Rollback: remove the host from the VirtualService and the tunnel ingress and drop the env; the
  client stays inert, the operator is unaffected, and no data was migrated.

## Evidence

- Host matrix: `crates/api/src/web.rs` tests and `crates/api/tests/web_hosts.rs` (game entry, deep
  link, asset, missing asset 404, config client id, no operator/design/MCP, operator unchanged,
  308 with query, hostile-Host redirect).
- Client: the realm Job log plus an admin-API read of `conveyor-game`, its redirect URIs, web
  origins, PKCE attribute and `conveyor` audience mapper.
- Live: `https://game.nonagentic.ai/`, `/portfolio`, `/config.json`, a missing asset 404, `/mcp`
  and `/design/*` 404, the operator origin unchanged, and the deployed OIDC sign-in as
  `conveyor-e2e` landing on the Portfolio read.

## Links

- Change request: `docs/plans/inception/GEMBA-WALK/08-change-request-001-host-and-identity.md`
- ADR 0021: `0021-gemba-walk-client.md`
- ADR 0022: `0022-bevy-ecs-world-layer.md`
- ADR 0023: `0023-gemba-walk-renderer.md`
- Spec: `docs/design/GEMBA-WALK-SPEC.md` §4, §5, §9, §10, §11
- Plan: `docs/plans/GEMBA-WALK.md` §7, §8, §10
- Register: `docs/plans/inception/GEMBA-WALK/03-artifact-register.md`
