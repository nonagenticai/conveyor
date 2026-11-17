# 0005 — Absorption sequence, strata stays external, and conveyor-executed rewrites

Status: Accepted · Date: 2026-11-13 · Amended: 2026-11-16 (see the amendment below) · Issue: #66 ·
Amendment: #243

## Decision

1. **The absorption sequence is fixed:** inference-router (#44) → licenses (#45) → billing-service
   (#46). The V2 product spine (T1–T3, T13) is queued behind them; it is not the current thrust.
   *(The queue in this decision is superseded by the 2026-11-16 amendment; the text stays as the
   historical record.)*
2. **`strata_ingest` and `strata_rag` stay separate repositories.** They are already Rust, they are
   shared with the agent platform, and their release cadence and open-sourcing are decided
   separately. The conveyor binds them over HTTP. This supersedes ADR 0004's inclusion of those two
   services in this repository.
3. **The `coding-*` fleet and the w2v services are rewritten by the conveyor itself**, not by
   hand-built absorption lanes. They are programme goals, not a queue of manual rewrites: T13 (tool
   platform) and T6 (development project) are what enable them, and the first proof is one service
   rewritten end to end by a conveyor job. No absorption issue is opened for them until that proof
   exists.
4. **The public tracker is the only tracker.** Internal programme work stays in the public issue
   queue, sanitized: no hostname, address, private repository slug or private path in an issue body
   or comment. No private programme repository is created.

## Amendment 2026-11-16 — the sequence ends at #45; the V2 spine is unblocked

Accepted by the owner on 2026-11-16; recorded as issue #243.

Measured: inference-router (#44) and licenses (#45) are merged. On 2026-11-16 the owner re-scoped
billing-service (#46): the parity port and the shared-Service cutover are dropped, nothing was
deployed, and the Python billing keeps serving the internal apps (issue #46, owner decision
`2026-11-16T22:11:32Z`). #46 now delivers GPU-surface usage — the price catalogue, read-time cost,
the project dimension and the screen — which is a programme feature, not an absorption.

Decision 1's queue therefore has no premise left. The absorption sequence is **complete at #45**;
the V2 product spine (T1–T3, T13) is no longer held behind #46 and may be claimed. The decision text
above and the consequences below stay as the historical record; this paragraph supersedes only
their queue.

Effect: #72 (T1) drops its `blocked-by #46` edge and becomes `status:ready`; #73 (T2), #74 (T3) and
#76 (T13) keep their real predecessors (#72, #73) and lose the superseded #46 citation. No other
ADR decision changes.

## Context

ADR 0004 defined the tool estate and said every in-scope service joins this repository when it is
rewritten in Rust. That stands for the services the conveyor calls today. It did not decide **who
performs** a rewrite, and it put two services in scope that are shared with another product and are
already Rust.

Measured 2026-11-13: `strata_ingest` and `strata_rag` are Rust and private, and both the agent
platform and the conveyor use them. The coding fleet is public Python behind `coding-gateway`; the
w2v fleet is private Python plus a TypeScript frontend.

## What this supersedes

- ADR 0004's in-scope list: `strata_ingest` and `strata_rag` remain **tool-estate** members but are
  not repository scope. Every other entry in 0004 stands.
- ADR 0004's "how a service joins" procedure is unchanged for the hand-performed absorptions
  (#44–#46). It does not describe the conveyor-executed rewrites, which follow the T6 development
  project path and land in this repository when the conveyor produces them.

## Consequences

- ~~After #44 the next two units are #45 and #46; the product spine waits.~~ Superseded by the
  2026-11-16 amendment: the sequence ends at #45 and the spine is unblocked.
- The two strata services keep their own deploys, and their source stays private until a separate
  open-sourcing decision.
- The coding and w2v rewrites appear in the programme as goals with named enabling slices, not as
  issues that can be claimed today.
- Issue hygiene becomes part of the delivery protocol: checked at creation and reported by the
  sweep.
