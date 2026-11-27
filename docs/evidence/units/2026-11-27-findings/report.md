### Evaluation — corpus `a7cf1a1aa2048be2…`

Window: nonagenticai/conveyor merged PRs and unit issues from 2026-11-11; collected 2026-11-27T17:00:47.591900+00:00 at `bd78baf1490dea9fa329e12a3355bfeec96763ed`. Spec `docs/plans/03-DELIVERY.md` `a15d13a40430a7f5…`; model `378a1395310416cd…`.

Completed units **330**; in flight **24** open (no merged PR — the delivery has not happened); closed without a keyword-linked PR 31; orphan PRs 11; automation excluded 129.

**Findings.** One finding per defect; a unit may carry several.

| Class | Count |
|---|---|
| `branch-live` | 1 |
| `closure-open` | 2 |
| `issue-unreadable` | 0 |
| `multi-lane` | 5 |
| `non-unit-dor` | 8 |
| `orphan-pr` | 11 |
| `unit-dor-verify` | 28 |

Orphan sub-kinds: `no-issue` 8, `unlinked-unit` 3.

- `branch-live`: `nonagenticai/conveyor#528`
- `closure-open`: `nonagenticai/conveyor#12`, `nonagenticai/conveyor#669`
- `multi-lane`: `nonagenticai/conveyor#185`, `nonagenticai/conveyor#190`, `nonagenticai/conveyor#212`, `nonagenticai/conveyor#319`, `nonagenticai/conveyor#81`
- `non-unit-dor`: `nonagenticai/conveyor#167`, `nonagenticai/conveyor#184`, `nonagenticai/conveyor#196`, `nonagenticai/conveyor#235`, `nonagenticai/conveyor#323`, `nonagenticai/conveyor#601`, `nonagenticai/conveyor#669`, `nonagenticai/conveyor#803`
- `orphan-pr`: `nonagenticai/conveyor#pr-335`, `nonagenticai/conveyor#pr-465`, `nonagenticai/conveyor#pr-493`, `nonagenticai/conveyor#pr-495`, `nonagenticai/conveyor#pr-578`, `nonagenticai/conveyor#pr-591`, `nonagenticai/conveyor#pr-597`, `nonagenticai/conveyor#pr-613`, `nonagenticai/conveyor#pr-64`, `nonagenticai/conveyor#pr-774`, `nonagenticai/conveyor#pr-96`
- `unit-dor-verify`: `nonagenticai/conveyor#11`, `nonagenticai/conveyor#130`, `nonagenticai/conveyor#140`, `nonagenticai/conveyor#203`, `nonagenticai/conveyor#205`, `nonagenticai/conveyor#212`, `nonagenticai/conveyor#213`, `nonagenticai/conveyor#226`, `nonagenticai/conveyor#274`, `nonagenticai/conveyor#299`, `nonagenticai/conveyor#307`, `nonagenticai/conveyor#311`, `nonagenticai/conveyor#389`, `nonagenticai/conveyor#390`, `nonagenticai/conveyor#416`, `nonagenticai/conveyor#434`, `nonagenticai/conveyor#45`, `nonagenticai/conveyor#471`, `nonagenticai/conveyor#542`, `nonagenticai/conveyor#545`, `nonagenticai/conveyor#572`, `nonagenticai/conveyor#59`, `nonagenticai/conveyor#62`, `nonagenticai/conveyor#653`, `nonagenticai/conveyor#667`, `nonagenticai/conveyor#73`, `nonagenticai/conveyor#797`, `nonagenticai/conveyor#92`

| Rule | Failing units |
|---|---|
| `ciBudget` | 0 |
| `closesKeyword` | 11 |
| `mergeComplete` | 14 |
| `oneBranch` | 4 |
| `onePull` | 5 |
| `pullUsable` | 0 |
| `verifyParts` | 47 |

| Chunk | Units | Verdict | Certificate |
|---|---|---|---|
| `chunk-000` | 200 | unproven | `01a21df5-1d01-71f8-a343-1f8a3f40272b` |
| `chunk-001` | 130 | unproven | `01a21df5-2d86-7ce1-a406-2a03119cd367` |

Programme bundle `01a21df5-3f9d-7f7a-bf50-55230c686bc3` (proven).

**53 completed unit(s) fail the model.**

| Unit | Failed rules |
|---|---|
| `nonagenticai/conveyor#11` | verifyParts |
| `nonagenticai/conveyor#12` | mergeComplete |
| `nonagenticai/conveyor#130` | verifyParts |
| `nonagenticai/conveyor#140` | verifyParts |
| `nonagenticai/conveyor#167` | verifyParts |
| `nonagenticai/conveyor#184` | verifyParts |
| `nonagenticai/conveyor#185` | onePull |
| `nonagenticai/conveyor#190` | oneBranch, onePull |
| `nonagenticai/conveyor#196` | verifyParts |
| `nonagenticai/conveyor#203` | verifyParts |
| `nonagenticai/conveyor#205` | verifyParts |
| `nonagenticai/conveyor#212` | oneBranch, onePull, verifyParts |
| `nonagenticai/conveyor#213` | verifyParts |
| `nonagenticai/conveyor#226` | verifyParts |
| `nonagenticai/conveyor#235` | verifyParts |
| `nonagenticai/conveyor#274` | verifyParts |
| `nonagenticai/conveyor#299` | verifyParts |
| `nonagenticai/conveyor#307` | verifyParts |
| `nonagenticai/conveyor#311` | verifyParts |
| `nonagenticai/conveyor#319` | oneBranch, onePull |
| `nonagenticai/conveyor#323` | verifyParts |
| `nonagenticai/conveyor#389` | verifyParts |
| `nonagenticai/conveyor#390` | verifyParts |
| `nonagenticai/conveyor#416` | verifyParts |
| `nonagenticai/conveyor#434` | verifyParts |
| `nonagenticai/conveyor#45` | verifyParts |
| `nonagenticai/conveyor#471` | verifyParts |
| `nonagenticai/conveyor#528` | mergeComplete |
| `nonagenticai/conveyor#542` | verifyParts |
| `nonagenticai/conveyor#545` | verifyParts |
| `nonagenticai/conveyor#572` | verifyParts |
| `nonagenticai/conveyor#59` | verifyParts |
| `nonagenticai/conveyor#601` | verifyParts |
| `nonagenticai/conveyor#62` | verifyParts |
| `nonagenticai/conveyor#653` | verifyParts |
| `nonagenticai/conveyor#667` | verifyParts |
| `nonagenticai/conveyor#669` | mergeComplete, verifyParts |
| `nonagenticai/conveyor#73` | verifyParts |
| `nonagenticai/conveyor#797` | verifyParts |
| `nonagenticai/conveyor#803` | verifyParts |
| `nonagenticai/conveyor#81` | oneBranch, onePull |
| `nonagenticai/conveyor#92` | verifyParts |
| `nonagenticai/conveyor#pr-335` | closesKeyword, mergeComplete, verifyParts |
| `nonagenticai/conveyor#pr-465` | closesKeyword, mergeComplete, verifyParts |
| `nonagenticai/conveyor#pr-493` | closesKeyword, mergeComplete, verifyParts |
| `nonagenticai/conveyor#pr-495` | closesKeyword, mergeComplete, verifyParts |
| `nonagenticai/conveyor#pr-578` | closesKeyword, mergeComplete, verifyParts |
| `nonagenticai/conveyor#pr-591` | closesKeyword, mergeComplete, verifyParts |
| `nonagenticai/conveyor#pr-597` | closesKeyword, mergeComplete, verifyParts |
| `nonagenticai/conveyor#pr-613` | closesKeyword, mergeComplete, verifyParts |
| `nonagenticai/conveyor#pr-64` | closesKeyword, mergeComplete, verifyParts |
| `nonagenticai/conveyor#pr-774` | closesKeyword, mergeComplete, verifyParts |
| `nonagenticai/conveyor#pr-96` | closesKeyword, mergeComplete, verifyParts |

Rules the snapshot cannot reconstruct are named, not dropped: claim timing/comments (the four pre-branch checks are not a property of the finished unit) and `Blocker:` iff `status:blocked` (label state is not historical).

**`Verify:` cross-check.** The fact reuses `scripts/ci/board.py`'s `VERIFY_RE` (the line rule `issue-dor` runs) and requires one per `###` part, which is what 03-DELIVERY's DoR says. The checker now enforces the same per-part rule, so a disagreement here means a pre-2026-11-18 body or a heading the model accepts loosely (for example `## Parts`). Disagreements: 0 pass the checker but fail the model, 6 the reverse, 313 agree. Both answers are recorded; neither is silently adopted.

The kernel certifies *facts ⇒ verdict*; the collector's snapshot, rule ids and artefact digests are what a reviewer checks against GitHub. The snapshot is redacted per `scripts/ci/no-private-refs.sh` (public GitHub quotations of estate identifiers become placeholders; the facts need none of them). No unit is attributed to a person.
