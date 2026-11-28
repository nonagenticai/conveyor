# Tranche 6 review — evidence

- Tranche: 01a1e9e1-07d3-75d3-8c99-4002d5bfe8f7 (tranche 6), programme 01a1d039-0d24-7d16-ae52-4841e96ccfac.
- Lane head: cdd32ea2e951f937472c90ad513d4c4a42c75cff.
- Snapshot manifest sha256: bb6bb79bfe00d5c943902a08715b3dc9e86d710c75c59092bfdb17bbe515563d.
- Exit gate: every member green and fresh at cdd32ea2e — S06a 5/5, S06b 18/18, S10 18/18,
  S11 18/18, T19F 17/17; `scripts/check-tranche-exit.py --tranche 6` exits 0.
- Evidence: the three approved programme baselines; the seven benefit profiles; the eleven member
  risks S06b-R1/R2, S10-R1–R4, S11-R1–R3, T19F-R1/R2; the programme risks R1–R14.
- Finding resolved: v1-u5-copilot-gate-quorum by 643077759 (the proposal opens at the tenant
  policy's quorum; the invoke gate refuses an approval below it; test
  `crates/api/tests/copilot_destructive.rs::a_policy_quorum_reaches_the_proposal_and_the_gate`).
- T19F closure (deferred to this review by its manifest): closure gate
  01a2211c-d49b-7f51-a176-ec8da438779d, destructive gate 01a2211d-0a73-7ca3-95db-817d70dd91bd,
  closure asset 01a2211c-d496-7ea7-9cb5-53cf7c30c93b; project state `closed`.
- Gate: 01a2211d-a05a-7e23-9145-0d3266a1341c, opened by conveyor-e2e and approved by
  conveyor-approver under standing delegation 01a2021d-cca2-757c-bfc2-21bd4e4b6d85.
- Review: 01a2211d-bbb6-7238-9bae-e29bf0b4f62d, decision continue; tranche 6 closed; tranche 7
  (01a1e9e1-0fab-7068-99f2-a233045b1276) authorised.
- Document: 01a2211d-81a4-7290-8590-396447533814, category tranche-review, published,
  content sha256 314d32959f4b7af56fbd6e99df4950cbf6cbcfacaad7294e771acc7dec8849c9.
