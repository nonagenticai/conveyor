# 0004 — The tool estate is in scope: one public monorepo

Status: Accepted · Date: 2026-11-12 · Issue: #42

## Decision

`conveyor` is one public monorepo. The platform and every tool it uses are written in Rust and
live in this workspace. A legacy service stays external and bindable as a toolkit until it is
rewritten, and joins the workspace on the day it is rewritten.

**In scope for this repository:**

- `inference-router`
- `licenses`
- `billing-service`
- the `coding-*` fleet (behind `coding-gateway`)
- the w2v services and `video-compiler`
- `strata_ingest`, `strata_rag`

**Out of scope — they stay their own products and their own repositories:**

- the agent platform and `nonagenticai_mcp_server`
- `fr` / RM, Planka, Keycloak
- `care-bus`
- `license-authz` — ingress enforcement; it keeps calling `licenses` over HTTP
- `artifact_service` / `storage-api`, `provisioning`, `modality-gateway`

## Repo scope is not tool-estate membership

These are two different lists and must not be conflated:

- **Repo scope** — what code lives here. `inference-router` is in it.
- **Tool estate** — what a conveyor job may call through the broker. `inference-router` is
  infrastructure, not a job tool; `strata_rag` and `strata_ingest` are both.

Inclusion in this repository never grants a job a tool. The broker's bindings decide that.

## Shared services keep independent deployment

Several of the in-scope services are used by more than conveyor (the agent platform, care-bus, the
ingress). Repo placement must not couple their release cadence to conveyor's:

- each service keeps its own container image, published per service, with an immutable digest;
- consumers other than conveyor pin a digest and do not build from this repository;
- the `strata_*` images stay independently published for the agent and care-bus apps;
- one service = one database = one role.

## How a service joins

Every absorption uses the same contract; the work is one issue per service:

1. **Capture the live contract** — the served OpenAPI and a golden request/response set, pinned by
   date and image digest. Fixtures are synthetic; a public repository is never a fixture target.
2. **Rewrite in Rust** in `apps/<service>` — no line-port, and nothing bulk-imported from a private
   working tree.
3. **Prove parity in three layers** — an OpenAPI lock check, a golden replay, and a shadow diff over
   recorded live traffic with an explicit, justified tolerance list.
4. **Cut over by flag** — one environment variable per consumer; a regression is one flag away from
   the old path.
5. **Record evidence and decommission last** — the old repository is archived only when the last
   consumer is off it, never when conveyor alone has cut over.

## Licensing and contributions

Apache-2.0 (see `LICENSE`). Contributions are accepted under the same terms by DCO sign-off
(`git commit -s`); there is no CLA. Contributions are open to anyone, and required checks plus
maintainer review gate every merge.

## What this supersedes

`docs/SPEC.md` §2 and `parity/parity.yaml` (rows P-0006 onward) state that `licenses`,
`billing_service` and `inference-router` are "never absorbed". That was the V1 scope. The owner's
ruling of 2026-11-12 supersedes it for the services listed above.

This decision does **not** edit `docs/SPEC.md`, `docs/SPEC_V2.md` or `parity/parity.yaml`. V1 and
V2 remain the reference record; this ADR is the current scope. Where they disagree with this
document, this document wins.

`docs/SPEC_V2.md` D58 already states that "the conveyor is one large repo: the platform plus every
new or rewritten tool"; D54 and D60 add the w2v fleet and the AHTAPol/w2v tenancy. This ADR applies
that direction to the services conveyor calls today.

## Consequences

- The absorption backlog becomes ordinary units — one service each — instead of one programme-sized
  rewrite.
- The public CI grows a per-service image matrix; until that lands, the single `checks` job remains
  the fallback.
- The repository's dependency surface grows by the services' own dependencies, each under
  `cargo audit` and Dependabot.
- Services used by other products keep their own image and database; the monorepo does not become a
  distributed monolith.
