# Tranche 1 / PI 1 — the rework record (retrospective, 2026-11-21)

Delegation: `01a1ff1f-e4ba-72dc-9c1a-13049933cf14` (the U5 extension of the standing delegation).
Tranche: `01a1e9e0-d787-7f5c-b5ed-15b05f3dc531` (state `closed`; authorised by the tranche-0
`continue` review `01a1ed84-e111-7082-9fa8-7fe77ccc9024`).
PI period: `tranche-1`. Programme: `01a1d039-0d24-7d16-ae52-4841e96ccfac`.
Snapshot: `d367a4a992ca5338c82466e9d6ef308e62b01ba6`, generated `2026-11-21T19:48:06Z`.

## The batch (U5b)

| project | plan | status before | verdict | transcript |
|---|---|---|---|---|
| S02 | `docs/plans/slices/S02-skeleton.md` | active; issue #2 open | rework — **closed** on the landed P2 proof | `S02/transcript.jsonl` (74 acts across the first run and its resumed repairs) |
| T1 | `docs/SPEC_V2.md` §12 | closed | rework — re-closed through the one gate | `T1/transcript.jsonl` (70 acts) |
| T11 | `docs/SPEC_V2.md` §12 | closed | rework — re-closed through the one gate | `T11/transcript.jsonl` (70 acts) |

S02's acceptance was **not** met when the batch began: issue #2's P2 named
`apps/server/tests/jobs_lease_two_workers.rs`, which did not exist on main or the branch. The
batch lands it (commit `d367a4a9`; P2.1–P2.3; local PASS 11.878 s; preflight 44938697512), so the
acceptance is met at the branch head and issue #2 closes with the lane PR (`Closes #2`).

## The records each project carries

| project | SoAW | delivery contract | scope baseline | Phase-G review | demo credit |
|---|---|---|---|---|---|
| S02 | `01a1ff84-4425-75d9-8772-08e09cf71069` | `01a1ff85-bda3-7671-a5b1-42eae3b0e6ee` | `01a1ff85-faae-7c2e-b897-fe957985ca78` | `01a1ff87-19a4-783f-85c5-033d5f38cedf` | `01a1ff87-9e5c-7454-8f79-8b33ada96519` |
| T1 | `01a1ff89-02bd-7b6d-902e-ffa5902cc7f5` | `01a1ff8a-754f-70de-a63a-fa8d00596433` | `01a1ff8a-b4f0-7e6e-9cc7-4231076e1039` | `01a1ff8b-b3ac-7082-bc16-df2f97bd7f8d` | `01a1ff8c-29be-7604-a144-0bcc7a1ea107` |
| T11 | `01a1ff8d-a0ad-7a91-b026-d9a2575daa3f` | `01a1ff8f-08ee-7bc5-bb39-81304d03c4ef` | `01a1ff8f-47cb-78cf-9db8-d98d863e7dd0` | `01a1ff90-4e19-7bcf-acbf-3f29d980572e` | `01a1ff90-cd9e-7afc-afc7-ce5bac11a07f` |

Each project also carries its charter gate, seven approved A–D artefacts (bound to their standard
key), the project brief and evidence outputs, a published closure review, and the closure gate
decided by an approver other than its requester. Manifests at the same snapshot: S02 13/13, T1
14/14, T11 14/14 — green.

## The SAFe cadence

| artefact | id |
|---|---|
| PI cadence | `01a1ff73-df1f-78c5-8793-101de5a3dd60` |
| backlog refinement | `01a1ff74-8982-77b9-971e-e4517c55d09a` |
| system demo | `01a1ff74-9819-72dc-a1f2-e102c73e9672` |
| inspect and adapt | `01a1ff74-a4d5-773a-a565-4bb5f9153557` |
| PI plan (published) | `01a1ff76-1ac5-7318-9d4f-2e4b5af310f1` |
| PI objectives (published) | `01a1ff76-5199-7175-b850-c234588ff48f` |
| system demo output (published) | `01a1ffaa-a89b-72e9-8936-705d500a1ef6` |
| inspect-and-adapt output (published) | `01a1ffaa-e333-747f-80e4-3204c939471e` |

## The retro lessons (one per finding)

| finding | lesson |
|---|---|
| T1-no-contemporaneous-record | `01a1ff82-cc70-7b18-bb12-6501ac4239e4` |
| T11-no-calibrated-threshold | `01a1ff82-e836-7867-981f-4bdfcfa860f0` |
| S02-p2-test-absent | `01a1ff83-06d1-7ff1-84c4-1f0d7957efc7` |
| rework-stale-public-claims | `01a1ff83-23de-713d-8099-d3e9ec8d74ea` |
| harness-list-readback-422 | `01a1ff83-3f5c-7734-830a-c8bb6c45024f` |

## The programme risks recorded for the review

| risk | row |
|---|---|
| V2 acceptance (#88/T12) remains open | `01a1ffa9-032a-7792-98c3-a85aa9bad193` |
| The six benefit baselines are placeholders | `01a1ffa9-1b48-7f8b-974a-c8d8e5366922` |
| Tranches 5 and 10–13 cannot deliver on proposed projects | `01a1ffa9-3907-7c8d-a6c0-819281d6c050` |

## The tranche review and the ledger

| act | row |
|---|---|
| tranche-1 review (`continue`) gate | `01a1ffa9-76b2-7956-98ba-c1d0be651017` (approved by `conveyor-approver`) |
| tranche-1 review | `01a1ffa9-c06f-7921-9d6b-611d871ce6cc` |
| tranche 2 | now `authorised` by that review |
| conformance baseline v5 | `docs/evidence/conformance-baseline.json`; acceptance `01a1ffa8-2268-7333-b189-a665cf5652bb` |

The audit at the batch snapshot reports **182 P0/P1** (27 P0 / 155 P1) and 3 P2. The batch deleted
**14** entries (3 P0, 11 P1) and added one P2 — `seed_live_drift` for S02, because the definition
seed still says active while the live record closes it; the seed is the definition image, not the
record. Strict audit: **OK**.

## What the batch found and fixed (instrument defects)

1. The Phase-G audit demanded a review against the **SoAW** as well as the delivery contract
   (`compliance_review_missing` for T1/T11); fixed to review the delivery contract, with a
   self-test that a SoAW-only review does not satisfy it.
2. The rework runner skipped an expected output when a row existed, leaving stale public claims;
   it now corrects the body through unpublish → update → publish and re-runs the heading gate.
3. Created artefacts carried no type binding, so the audit could not resolve them; the runner now
   binds the standard key and re-versions an existing unbound row.
4. A resumed run treated the pre-reopen closure approval as current; the close gate counts only a
   closure taken after `reopened_at`, and the runner now does the same.
5. The content check crashed on a closing content without closure keys; it now refuses cleanly.
