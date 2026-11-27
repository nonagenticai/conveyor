### Findings — the live unit ledger, corpus `a7cf1a1aa2048be2…`

This is the evaluation run of
[#865](https://github.com/nonagenticai/conveyor/issues/865): the same collector, model and delivery
contract as the 11:00 run (`docs/evidence/units/2026-11-27/`), collected after the instrument
corrections in § The instrument's own findings. The snapshot was collected at
`2026-11-27T17:00:47Z` from the `issue/865-proof-findings` worktree at `bd78baf14` (origin/main
`949e95c8c` plus the claim marker); the spec and model are the origin/main bytes the digests name.

| Fact | Value |
|---|---|
| Corpus | `a7cf1a1aa2048be2b2d9c012becce257d191ac89cb26359756776671fc719c35` |
| Spec | `docs/plans/03-DELIVERY.md` `a15d13a40430a7f58c8b5c66e5deefdd6ff1073be0d283c8a58059abdeaac439` |
| Model | `docs/spec/lean/DeliveryModel.lean` `378a1395310416cd907870874d1c3e55f8d57b44c127018650c5b7fb574ddae7` |
| Collector | `scripts/proof/collect_units.py` `c3c9b55501a9307815d50264f997a4a1f94edb3c8033d8912609f909db10d6b2` |
| Snapshot artifacts | heads `2930cbf4705d…` · issues `34b7051b980e…` · pulls `ae8605b5e9b2…` |
| Engine | deployed `conveyor` image `sha256:cbee6af8a5ce…`, two pods Running; `lean4.token` minted `17:02:01Z`, expiring `18:02:02Z` |

The kernel certifies *facts ⇒ verdict* through the deployed engine:

| Certificate | Units | Verdict | Kernel | `certificate_sha256` |
|---|---|---|---|---|
| `01a21df5-1d01-71f8-a343-1f8a3f40272b` | 200 | `unproven` | `valid:false`, 2 667.76 ms | `2bf062a01ba56504…` |
| `01a21df5-2d86-7ce1-a406-2a03119cd367` | 130 | `unproven` | `valid:false`, 2 461.94 ms | `d1d179c5e387f0d7…` |
| `01a21df5-3f9d-7f7a-bf50-55230c686bc3` | bundle | `proven` | `valid:true` | `9ec7a32a34a51bfb…` |

Completed **330**; open in flight **24**; closed without a keyword-linked PR **31** (3 paired with
an unlinked PR); orphan PRs **11**; automation excluded **129**. **53 completed units fail the
model.** The findings below are **55**: one unit can carry several findings, and an orphan PR is
one defect, not three rule failures.

#### The findings

| Class | Count | Keys |
|---|---|---|
| `unit-dor-verify` | 28 | #11, #130, #140, #203, #205, #212, #213, #226, #274, #299, #307, #311, #389, #390, #416, #434, #45, #471, #542, #545, #572, #59, #62, #653, #667, #73, #797, #92 |
| `non-unit-dor` | 8 | #167, #184, #196, #235, #323, #601, #669, #803 |
| `multi-lane` | 5 | #81, #185, #190, #212, #319 |
| `closure-open` | 2 | #12, #669 |
| `branch-live` | 1 | #528 |
| `orphan-pr` | 11 | `unlinked-unit`: #pr-493→#470, #pr-495→#26, #pr-578→#577 · `no-issue`: #pr-64, #pr-96, #pr-335, #pr-465, #pr-591, #pr-597, #pr-613, #pr-774 |

- **`unit-dor-verify`** — a unit issue whose Parts section lacks a `Verify:` line on a part. All 28
  are closed and predate the template settling; they are the record, not work to repair.
- **`non-unit-dor`** — `verifyParts` fails but the issue is not a `unit` deliverable
  (bug/docs/ci/security/design); the Definition of Ready does not apply. The kernel still evaluates
  the rule because the model has no `unit` fact; the scoping fix is
  [#868](https://github.com/nonagenticai/conveyor/issues/868), blocked-by #837 on the same file.
- **`multi-lane`** — two live claim branches or two linked PRs for one unit.
- **`closure-open`** — the PR merged and the issue is still open. #12 and #669 are not stale: the
  owner reopened them on 2026-11-24 after a closure audit found the live proof missing. They are
  the ledger's true unfinished deliveries.
- **`branch-live`** — #528's issue is closed while its merged PR's head ref is live. The live ref is
  not the merged lineage (§ The live queue).
- **`orphan-pr`** — a merged PR with no closing issue: three are real deliveries whose PR body
  carried no closing keyword (each issue was closed by hand with the record), eight are genuine
  no-issue PRs. Console publish #494 (`publish/design-…`) is now excluded as automation, and the
  policy question for the eight is [#869](https://github.com/nonagenticai/conveyor/issues/869).

Rule totals (the collector's `local_rules` mirror; the kernel's `ruleResults` certifies the same
seven rules as booleans): `closesKeyword` 11, `mergeComplete` 14, `oneBranch` 4, `onePull` 5,
`verifyParts` 47; `ciBudget` and `pullUsable` 0. One unit carries both a `multi-lane` and a
`unit-dor-verify` finding (#212); the other four `multi-lane` units pass `verifyParts`.

#### The instrument's own findings (fixed here)

1. **"54 in flight" was 23 open plus 31 closed.** `build_units` did not filter state, so the 11:00
   report claimed "the delivery has not happened" for 31 closed issues. The corpus now separates
   `in_flight` (open) from `closed_unlinked`; three pair with an unlinked PR (#26/#470/#577), the
   rest closed without a keyword-linked conveyor PR (delivery in another repository, or no
   remaining work).
2. **`verifyParts` was applied to non-unit issues.** Eight failures are bug/docs/ci/security/design
   issues, outside the content contract 03-DELIVERY scopes to `unit` deliverables. They are
   `non-unit-dor`, not unit defects.
3. **Two `verifyParts` failures were extraction defects.** `contract_verify` ended the Parts section
   at any line starting with `#`, so prose references like `#8 · …` and `#791 hardens …` truncated
   #753's and #781's sections before later parts. A heading is hashes followed by whitespace; with
   the fix both units pass, and the rule failures drop 49 → 47 (the class 30 → 28).
4. **A console publish PR was counted as an orphan unit.** #494 is now excluded like `automation/*`.

The `Verify:` cross-check against `scripts/ci/board.py` after aligning the checker to one `Verify:`
per `###` part: **313 agree, 0 pass the checker and fail the model, 6 the reverse**. The six are
bodies headed `## Parts` or `## Parts and acceptance` (#111, #113, #151, #356, #604, #611) rather
than the template's exact `## Parts and acceptance criteria`: the model accepts the looser heading,
the checker requires the template's. Both answers are recorded, never reconciled silently.

#### The live queue (24 open units)

| State | Count | Keys |
|---|---|---|
| claimed | 10 | #865, #864, #862, #860, #853, #848, #837, #814, #755, #526 — each with a live `issue/<n>-*` branch |
| ready | 9 | #838, #816, #815, #783, #31, #29, #16, #9, #4 |
| blocked | 4 | #854, #852, #830, #817 |
| needs-human | 1 | #395 |

No claim is released here: 03-DELIVERY § Labels and states makes releasing a stale claim a human
decision, and the board already reports it. One unit carries two live branches:

- **#526** — `issue/526-tr2` @ `4e5d994f2` (101 commits ahead of main, no PR) and `issue/526-u5a`
  @ `f881ffffb` (905 ahead, PR #846 open). `oneBranch` counts distinct claim-branch names; completing
  with both live fails it. The finding is commented on the issue (comment 7223977805).

**#528's live branch is not a stale merged branch.** `issue/528-delivery-view` @ `e0629400` is 22
commits ahead of main and does not contain PR #530's merged head `e077f48f` — a parallel lineage
under a closed issue. It is left in place; adopting or archiving it is the owner's call. Deleting it
here would destroy work the record does not account for.

#### What this proves, and what it does not

The kernel certifies that the facts in this corpus do not satisfy `checkUnit`, and the passing
subset's live certification (`docs/evidence/units/2026-11-27/control/`) proves the same path accepts
a real proven set. The class assignment (`closure-open`, `orphan-pr/unlinked-unit`, …) is the
collector's deterministic classification of those facts, not a Lean theorem; its rules and the
snapshot's provenance are what a reviewer re-runs. No unit is attributed to a person, and a
certificate says nothing about whether the facts describe what happened — the committed snapshot is
the reviewer's lever, as in the method's assumption line.
