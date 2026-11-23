# DESIGN-PUBLISH — retrospective rework note (2026-11-21)

**Verdict:** rework (U3's verdict) — completed through the reconstituted lifecycle.

**Evidence found:** the published design pack, its data and evidence pages, the closure review and the declared outputs.

**Records created:** charter, Statement of Architecture Work, seven A–D artefacts approved by the
board, delivery contract (requester ≠ approver, delegation cited), project baseline with one WBS
deliverable, closure review output, Phase-G compliance review, closure gate and closure. The 37
acts are in `transcript.jsonl`; each write was read back by `programme-record.py`.

**Verification:** `python3 scripts/verify-project.py --project DESIGN-PUBLISH --snapshot <fresh>` is green
(11 checks, 0 failures) at `d7293f96`; the strict audit on the same snapshot carries no
P0/P1 entry for the project.

**Residuals:** the live public window is narrower than the committed pack (102 outputs live vs 165 in the pack); the republish from live rows is U7's, and the project-brief URL 404s live until then.
