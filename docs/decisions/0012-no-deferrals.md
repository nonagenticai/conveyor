# 0010 — No deferrals; every finding executes

Status: Accepted · Date: 2026-11-22 · Slice: #526 (programme)

## Context

The programme was disposing of open work with dated instruments rather than executing it:
`v1-s14-1` carried a waiver accepted 2026-11-22 and expiring 2027-02-18; the acceptance manifest
`acceptance/programme/P0.toml` carried an `[[allow]]` for `benefit_baseline` expiring 2027-02-18;
`verify-viability.py` refused the programme unless every open finding carried a valid deferral; and
`verify-project.py` and `audit-programme-conformance.py` suppressed non-conformance through active
waivers. The measured board at `58645942` shows what that bought: 12 green / 35 red manifests, 14
closed-on-red projects, 965 artefacts of which 418 are absent and 311 partial, 483 standards
findings, 102 audit packages, 62 contract gaps, and 6 of 6 viability checks missing.

On 2026-11-22 the owner directed that **all dated deferrals are no longer true: the programme defers
nothing; it executes the work**, and that decision and approval authority sits with the Claude
sessions, exercised with rigour, grounding, and captured and published evidence.

## Decision

1. No deferral, waiver, `[[allow]]` or dated residual is a valid disposition for any #526 finding or
   artefact. Every finding is executed.
2. Where a proof cannot exist because the system it proved has been decommissioned, the finding's
   **claim is corrected** to what the surviving system can prove, with the decommissioning as its
   evidence. A correction is not a waiver and has no expiry.
3. The instruments refuse suppression: `verify-viability.py`, `verify-project.py`,
   `audit-programme-conformance.py` and `governance/conformance.json` treat a waiver or deferral on
   programme work as a finding. The TOGAF `architecture_waiver` remains legal for a deviation from
   an architecture specification (SPEC_V2 §8.1), requires an accepting owner and a reassessment date,
   and the programme carries zero active ones at green.
4. Decisions are `rule` decisions with a resolving `via.delegation_ref`, published with their
   rationale and evidence. No gate is claimed as human.
5. The superseded decision and its register row are kept; history is forward-only.

## Consequences

- The findings ledger re-anchors `v1-s14-1` to decision document
  `01a202a6-c20c-7079-9e0e-488009d21e86`; its waiver is removed and the finding stays open until it
  is executed or its claim corrected.
- Removing suppression turns the gates red-first (98 open findings, 539 packages). The gate stays red
  until the work clears it; no green is claimed in the meantime.
- Every tranche review from tranche 3 cites this ADR for its no-deferral verdict.
- The acceptance standard (`v5 §5`, clauses (a)–(j)) is the close gate, and its own change row
  `U5-ACCEPTANCE-STANDARD` (`01a2029f-8e2e-78de-bc99-119da42fdfbf`) stays open until the `change`
  gate can be approved (blocked by `v1-change-1`).

## Evidence

- Decision document `01a202a6-c20c-7079-9e0e-488009d21e86` (published text compose, category
  `decision`, unit `U5`, project PLATFORM — the pack-indexed copy; the byte-backed issues
  `01a202a3-76b1-72da-8a41-7a1662b91fc7` and `01a2029c-9254-7605-8a7c-3013b28ee794` are
  unpublished and kept as history, `v1-outputs-1`).
- Register row `U5-NO-DEFERRALS` `01a2029f-5a5f-70aa-be13-029435893a40`.
- The measured board at `58645942`: issue #526, comment 7077202144.
- Plan of record `~/.claude/plans/which-project-in-the-noble-turing.md` v5, rulings R7–R9.

## Supersedes

`01a20208-5261-7085-a4a7-74f061e4842b` (decision document) and
`01a20208-0c30-7f4f-a3b7-ab1b467e2b46` (register row `U5-RESIDUAL-S14`).

## Not claimed

No finding is marked resolved by this ADR. Nothing is invented. The S14 receipts that the
decommissioned systems cannot produce are either executed against the surviving harness or corrected
to the surviving proof; they are never asserted as receipts.
