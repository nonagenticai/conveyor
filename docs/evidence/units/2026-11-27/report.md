### Evaluation — corpus `36439d192a460a9e…`

Window: nonagenticai/conveyor merged PRs and unit issues from 2026-11-11; collected 2026-11-27T11:00:09.975521+00:00 at `48797526cc3633e5fd7685919f35e35e10ffb988`. Spec `docs/plans/03-DELIVERY.md` `12514c99588f9b8c…`; model `378a1395310416cd…`.

Completed units **320**; in flight 54 (not certified — the delivery has not happened); orphan PRs 12; automation excluded 128.

| Rule | Failing units |
|---|---|
| `ciBudget` | 0 |
| `closesKeyword` | 12 |
| `mergeComplete` | 15 |
| `oneBranch` | 4 |
| `onePull` | 5 |
| `pullUsable` | 0 |
| `verifyParts` | 50 |

**56 completed unit(s) fail the model.**

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
| `nonagenticai/conveyor#753` | verifyParts |
| `nonagenticai/conveyor#781` | verifyParts |
| `nonagenticai/conveyor#797` | verifyParts |
| `nonagenticai/conveyor#803` | verifyParts |
| `nonagenticai/conveyor#81` | oneBranch, onePull |
| `nonagenticai/conveyor#92` | verifyParts |
| `nonagenticai/conveyor#pr-335` | closesKeyword, mergeComplete, verifyParts |
| `nonagenticai/conveyor#pr-465` | closesKeyword, mergeComplete, verifyParts |
| `nonagenticai/conveyor#pr-493` | closesKeyword, mergeComplete, verifyParts |
| `nonagenticai/conveyor#pr-494` | closesKeyword, mergeComplete, verifyParts |
| `nonagenticai/conveyor#pr-495` | closesKeyword, mergeComplete, verifyParts |
| `nonagenticai/conveyor#pr-578` | closesKeyword, mergeComplete, verifyParts |
| `nonagenticai/conveyor#pr-591` | closesKeyword, mergeComplete, verifyParts |
| `nonagenticai/conveyor#pr-597` | closesKeyword, mergeComplete, verifyParts |
| `nonagenticai/conveyor#pr-613` | closesKeyword, mergeComplete, verifyParts |
| `nonagenticai/conveyor#pr-64` | closesKeyword, mergeComplete, verifyParts |
| `nonagenticai/conveyor#pr-774` | closesKeyword, mergeComplete, verifyParts |
| `nonagenticai/conveyor#pr-96` | closesKeyword, mergeComplete, verifyParts |

Rules the snapshot cannot reconstruct are named, not dropped: claim timing/comments (the four pre-branch checks are not a property of the finished unit) and `Blocker:` iff `status:blocked` (label state is not historical).

**`Verify:` cross-check.** The fact reuses `scripts/ci/board.py`'s `VERIFY_RE` (the line rule `issue-dor` runs) and requires one per `###` part, which is what 03-DELIVERY's DoR says; the CI checker requires only one in the exact `Parts and acceptance criteria` section. Disagreements: 9 pass the checker but fail the model, 6 the reverse, 293 agree. Both answers are recorded; neither is silently adopted.

The kernel certifies *facts ⇒ verdict*; the collector's snapshot, rule ids and artefact digests are what a reviewer checks against GitHub. No unit is attributed to a person.
