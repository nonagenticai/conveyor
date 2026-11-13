# 0005 — Absorption sequence, strata stays external, and conveyor-executed rewrites

Status: Accepted · Date: 2026-11-13 · Issue: #66

## Decision

1. **The absorption sequence is fixed:** inference-router (#44) → licenses (#45) → billing-service
   (#46). The V2 product spine (T1–T3, T13) is queued behind them; it is not the current thrust.
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

- After #44 the next two units are #45 and #46; the product spine waits.
- The two strata services keep their own deploys, and their source stays private until a separate
  open-sourcing decision.
- The coding and w2v rewrites appear in the programme as goals with named enabling slices, not as
  issues that can be claimed today.
- Issue hygiene becomes part of the delivery protocol: checked at creation and reported by the
  sweep.
