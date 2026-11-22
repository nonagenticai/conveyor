# S04 — the Phase-G compliance review's evidence (2026-11-22)

This file exists **before** the review is recorded: the recorder refuses a write whose evidence path
does not already exist.

## What is recorded

`architecture.reviews_record` for S04's live governing contract
`01a20256-6ede-7d70-aae8-5dbfe681a1b7` (Statement of Architecture Work, phase A), level
**`compliant`**, under the published standing delegation `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`.
The review is not a clean bill: it records six deviations, none of them a P0.

## The measurement the review cites

- Lane head at measurement: `582e2dc67e5882eb63f40de67c129f8c2863c376`.
- S04's gate proof: the HTTP/MCP parity run, **7/7**, at that head (the action registry serves the
  same contract over both transports).
- Fresh full-fidelity snapshot at that head: **12 green / 35 red** manifests, strict audit 200 gaps,
  standards **919 artefacts / 456 findings**. Work table: issue #526 comment 7076904694.
- S04's own standards findings at that snapshot: compliance-review (P0, this record),
  delivery-contract (P1), project-registers (P2), architecture-artifact (P2).

## The six deviations

1. **Delivery contract not yet recorded** (P1). The SoAW governs phases A–G; the post-F delivery
   contract — delivery commitments and the method for assessing compliance — is recorded with this
   tranche's close.
2. **Project registers partial** (P2). The register set does not yet carry all six kinds with named
   owners; the missing kinds are added in this tranche.
3. **`architecture-artifact` sections partial** (P2). The A–D artefacts exist and are approved, but
   the standard's required sections for the instance do not all resolve (metadata and document
   linkage).
4. **The plan frontmatter said `planned` while the code and the parity proof are live.** Corrected
   with evidence in the same record rather than closing against a stale plan.
5. **The gate clause was not executed by the manifest.** The 7/7 parity run is the proof; it is
   encoded as the manifest's gate clause under the acceptance-standard change.
6. **The mandatory set for S04 is not yet complete** — this review closes the compliance-review P0;
   the other three findings are dated packages, not waivers.

## Read-back

The recorder refreshes the snapshot after the write and reads the review row back from
`architecture.json`'s `compliance_reviews` by the id the action returns.
