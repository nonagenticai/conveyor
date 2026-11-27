---
document: GEMBA-WALK/change-request-001
version: 1
status: proposed
project: GEMBA-WALK
category: change-request
owner: programme-architect
reviewer: Architecture Board
review_date: 2026-11-27
change: CR-WALK-001
---

# GEMBA-WALK — Change request 001: the game host and its own identity

## Change requests

Change the accepted delivery architecture for the Gemba Walk from **a `/walk/` sub-site on the
operator origin using the `conveyor-web` client, with production sign-in deferred**, to **a
first-class client served at `https://game.nonagentic.ai/` with its own public Keycloak client
`conveyor-game`**, while keeping one image, one service, one API and one principal set.

The owner's direction of 2026-11-27 is the driver: the game is a secondary UI that may become the
primary UI, it needs its own Keycloak setup and its own URL, and the UI must work first time.

This request changes the scope of G0 (#813), the frozen route model (W0), the serving architecture
(SPEC §4/§10), the identity contract (ADR 0021 decision 4), and the governing decisions D67 and
§10.5.11. It does **not** change the parity goal, the action registry, the data model, the
`apps/web` client, or the primacy decision (P6).

## 2. Why the accepted design no longer meets the direction

| Accepted position | Source | Why it falls short |
|---|---|---|
| static sub-site at `/walk/`, same image and origin | ADR 0021 decision 1; SPEC §4, §10 | a sub-path on the operator origin cannot be the game's own product URL; a later promotion would require a second move |
| the same public client with a `/walk/` redirect URI, or a dedicated client "if the identity operator prefers isolation" | ADR 0021 decision 4; SPEC §10 | leaves the identity question open; the owner has now chosen isolation; a production sign-in is not provable on the operator client's redirect set |
| production sign-in is `acceptance-blocked-by` an external redirect registration | issue #813; plan §7 | the owner has made the identity setup part of the work; an acceptance-blocked scenario is not "works first time" |
| the walk path is `/walk` + app path | plan §4; census | a game host at the root makes parity an equality; keeping the prefix on the new host would ship an artificial URL |

## 3. Options considered

| Option | Shape | Assessment |
|---|---|---|
| **A. Keep the sub-site; add a redirect URI** | `https://conveyor.nonagentic.ai/walk/`, `conveyor-web` | Rejected. It does not give the game its own URL or its own client, and the owner's direction names both. It also makes a later primacy move a second migration. |
| **B. New realm, new deployment, new host** | realm `game`, its own service and image, `game.nonagentic.ai` | Rejected. A second realm duplicates the user set and breaks single sign-on; a second deployment doubles the runtime, secrets, probes, image and CI surface for no capability. The estate's realm policy is one realm per app, and `conveyor` already is that realm. |
| **C. Same image and service, second host, dedicated client in the same realm** | `conveyor-public-vs` gains `game.nonagentic.ai`; the server dispatches on `Host`; Keycloak gains public client `conveyor-game` in realm `conveyor`; the API accepts the two-client `azp` allowlist | **Chosen.** It gives the game its own URL, session client and redirect policy with no new runtime, no CORS (the API is same-origin on both hosts), no duplicated users and a reversible rollout. |

## Impact

### 4.1 Scope

- **W0 (census and instrument)** is amended: the census's `walk` field becomes the host-rooted app
  path (`/portfolio`, not `/walk/portfolio`; the entrance is `/`). The route and action counts stay
  52 / 598 at the re-baseline. The instrument's claim checks are unchanged; only the key changes.
- **W2 is split** into W2a (serving and identity), W2b (the client), W2c (tests and CI). G0's
  parts gain the host matrix, the identity acceptance and the live proof.
- **A second repository change** is part of the unit: `gitops-production` gains the Keycloak client,
  the tunnel ingress, the VirtualService host and the deployment env. It is one PR in that repo,
  delivered with the same evidence discipline.
- **Out of scope remains**: S1–S4, any new table/action/event/read route, a second realm, a second
  deployment, a new port, primacy, and any production data write.

### 4.2 Schedule

No calendar change. The stage order changes: the change record and the delegated decision are
recorded **before** the build; the conveyor image is deployed **before** the game host is routed;
the realm Job runs **before** the live sign-in proof.

### 4.3 Cost

| Item | Cost |
|---|---|
| Runtime | none: the same process, image, service and database; the second host is another VirtualService host |
| Keycloak | one public client and one audience mapper in an existing realm; the manual realm Job re-run |
| Build/CI | one wasm build in the existing image stage and the walk workflow; no new runner class |
| Test | the host matrix (Rust integration) and one deployed OIDC spec; the dev suite is unchanged |
| Operations | one more hostname in the tunnel ConfigMap and one more line in the dns-ensure-derived DNS; no cert or gateway change |

### 4.4 Benefit

B8 remains the project's enabling benefit; this change does not claim a measured benefit. The
enabling effects are concrete: a product URL the owner named; a separate client session and
redirect policy; a production sign-in that can be proven; and a first-class path to the P6 primacy
decision without a later migration.

### 4.5 Risk

| Change to the register | Detail |
|---|---|
| R-WALK-4 closed | the OIDC redirect/client is no longer external; it is delivered and proven in this unit |
| R-WALK-6 added | the game host could serve an operator surface (MCP, design, operator index); controlled by a router-level host gate, a host-aware static dispatch, `app()` integration tests and live 404 probes |
| R-WALK-7 added | the `azp` allowlist could widen the replay guard or default the game id from the audience; controlled by an explicit optional field, a two-id allowlist and rejection tests |
| R-WALK-8 added | the Keycloak client could drift or the manual Job could be skipped; controlled by a git-managed rep, a create-or-refuse preflight and an admin-API read-back |
| R-WALK-9 added | DNS, tunnel or VirtualService lag could make the host unavailable after the merge; controlled by the dns-ensure read-back and the live probes |

### 4.6 Affected contracts and decisions

- **SoAW v2** — the Phase-A assignment now includes the host and the production identity; the
  contract is superseded, not edited in place.
- **Delivery contract (Phase F)** — created with the amended commitments and the host/identity
  evidence method.
- **SPEC_V2 D68** — supersedes D67 in part; §10.5.11's serving sentence is amended with a pointer.
- **Typed artefacts** — `solution_concept`, `requirements_catalog` and `technology_standards`
  supersede to v3; `data_entity_catalog` is unchanged (no new entity).
- **ADR 0021 decisions 1 and 4** — superseded in part by ADR 0024; the rest stands.

## Decisions

**Approved as a delegated rule decision under the published delegation of 2026-11-22 (reference
`01a1fb9f-ac2d-732d-b274-1079be3dd2b3`; expiry 2027-02-18) — not a human signature.** The
Architecture Board decision is recorded live with that reference; the SoAW v2 and the delivery
contract are approved by a distinct approver. The owner's direction is the request's driver; the
delegation is what permits the routine gate to be decided without a human signature.

## 6. Implementation order

1. This request and ADR 0024 are committed; the affected documents are versioned.
2. The live `changes.request`, Board decision, SoAW v2 supersession and delivery contract are
   recorded and read back.
3. The gitops change is prepared and reviewed; it is **not merged** yet.
4. The conveyor service and identity code and the walk client are built, tested and merged; the
   image is deployed with the game host configured but **not routed**.
5. The gitops change is merged; Flux applies the env, the tunnel ingress and the VirtualService
   host; the realm Job is run; the client and mapper are read back from the admin API.
6. The live host and the real sign-in are proven; the evidence is recorded; the issue closes.

## 7. Acceptance evidence

| Scenario | Evidence |
|---|---|
| The census is re-keyed and the instrument still checks claims | `python3 scripts/build-walk-coverage.py --check` + `--self-test` + the re-frozen census |
| The game host serves only the walk and the API | Rust host-matrix tests through `app()`, including `/mcp` and `/design` 404 |
| The operator host is unchanged and redirects the legacy path | the operator tests + the `/walk/portfolio` 308 |
| The Keycloak client exists and is correct | the realm Job log + an admin-API read of `conveyor-game`, its redirect URIs, web origins, PKCE and the `conveyor` audience mapper |
| A real production sign-in works | the deployed Playwright OIDC spec as `conveyor-e2e`, the Portfolio read and `/api/me` 200 |
| The change is governed | the live change row, the Board decision, SoAW v2, the delivery contract and the document read-backs |

## 8. Rollback

Remove `game.nonagentic.ai` from `conveyor-public-vs` and the cloudflared ingress, and drop the two
deployment env values; the service and the operator origin are unaffected, and the Keycloak client
stays inert. The walk tree and the `/walk/*` 308 can be removed in a follow-up if the direction is
reversed; no data is migrated, so there is nothing to roll back in the database. A rollback is a
change request with its own evidence.

## 9. Links

- ADR 0024: `../../../decisions/0024-gemba-walk-host-and-identity.md`
- ADR 0021: `../../../decisions/0021-gemba-walk-client.md`
- Spec: `../../../design/GEMBA-WALK-SPEC.md` §4, §9, §10, §11
- Plan: `../../../plans/GEMBA-WALK.md` §7, §8, §10
- WBS/registers: `07-governance-and-evidence.md`
- Artefact register: `03-artifact-register.md`
- Issue: #813 (G0); S1–S4 #814–#817
- Delegation: `01a1fb9f-ac2d-732d-b274-1079be3dd2b3` (expiry 2027-02-18)
