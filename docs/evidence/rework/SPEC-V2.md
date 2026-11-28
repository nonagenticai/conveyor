# SPEC-V2 — retrospective rework note (2026-11-21)

**Verdict:** rework (U3's verdict) — completed through the reconstituted lifecycle.

**Evidence found:** `docs/SPEC_V2.md` (170 KB) published as the `spec` output, the adopted standard and the decisions D1–D63.

**Records created:** charter, Statement of Architecture Work, seven A–D artefacts approved by the
board, delivery contract (requester ≠ approver, delegation cited), project baseline with one WBS
deliverable, closure review output, Phase-G compliance review, closure gate and closure. The 38
acts are in `transcript.jsonl`; each write was read back by `programme-record.py`.

**Verification:** `python3 scripts/verify-project.py --project SPEC-V2 --snapshot <fresh>` is green
(11 checks, 0 failures) at `d7293f96`; the strict audit on the same snapshot carries no
P0/P1 entry for the project.

**Residuals:** who closed the project on 2026-11-17 and why is not recoverable (no approval row or closure asset); the contemporaneous WSJF inputs and B1's baseline value are not recoverable.
