### Evaluation — corpus `f5fc2555eade8e8b…`

Window: nonagenticai/conveyor merged PRs and unit issues from 2026-11-11; collected 2026-11-27T22:25:45.905827+00:00 at `ef382c8df22926a2611ef538e9b526eda936185a`. Spec `docs/plans/03-DELIVERY.md` `9a5fa2cedebd6fbb…`; model `f6020faea6420ed1…`.

Completed units **345**; in flight **17** open (no merged PR — the delivery has not happened); closed without a keyword-linked PR 32; orphan PRs 3; automation excluded 130.

**Findings.** One finding per defect; a unit may carry several.

| Class | Count |
|---|---|
| `branch-live` | 0 |
| `closure-open` | 2 |
| `issue-unreadable` | 0 |
| `multi-lane` | 5 |
| `non-unit-dor` | 0 |
| `orphan-pr` | 3 |
| `unit-dor-verify` | 28 |

Orphan sub-kinds: `unlinked-unit` 3.

- `closure-open`: `nonagenticai/conveyor#12`, `nonagenticai/conveyor#669`
- `multi-lane`: `nonagenticai/conveyor#185`, `nonagenticai/conveyor#190`, `nonagenticai/conveyor#212`, `nonagenticai/conveyor#319`, `nonagenticai/conveyor#81`
- `orphan-pr`: `nonagenticai/conveyor#pr-493`, `nonagenticai/conveyor#pr-495`, `nonagenticai/conveyor#pr-578`
- `unit-dor-verify`: `nonagenticai/conveyor#11`, `nonagenticai/conveyor#130`, `nonagenticai/conveyor#140`, `nonagenticai/conveyor#203`, `nonagenticai/conveyor#205`, `nonagenticai/conveyor#212`, `nonagenticai/conveyor#213`, `nonagenticai/conveyor#226`, `nonagenticai/conveyor#274`, `nonagenticai/conveyor#299`, `nonagenticai/conveyor#307`, `nonagenticai/conveyor#311`, `nonagenticai/conveyor#389`, `nonagenticai/conveyor#390`, `nonagenticai/conveyor#416`, `nonagenticai/conveyor#434`, `nonagenticai/conveyor#45`, `nonagenticai/conveyor#471`, `nonagenticai/conveyor#542`, `nonagenticai/conveyor#545`, `nonagenticai/conveyor#572`, `nonagenticai/conveyor#59`, `nonagenticai/conveyor#62`, `nonagenticai/conveyor#653`, `nonagenticai/conveyor#667`, `nonagenticai/conveyor#73`, `nonagenticai/conveyor#797`, `nonagenticai/conveyor#92`

| Rule | Failing units |
|---|---|
| `ciBudget` | 0 |
| `closesKeyword` | 3 |
| `mergeComplete` | 5 |
| `oneBranch` | 4 |
| `onePull` | 5 |
| `pullUsable` | 0 |
| `verifyParts` | 31 |

**37 completed unit(s) fail the collector's rule mirror; the kernel certificate is pending the deployed engine.**

| Unit | Failed rules |
|---|---|
| `nonagenticai/conveyor#11` | verifyParts |
| `nonagenticai/conveyor#12` | mergeComplete |
| `nonagenticai/conveyor#130` | verifyParts |
| `nonagenticai/conveyor#140` | verifyParts |
| `nonagenticai/conveyor#185` | onePull |
| `nonagenticai/conveyor#190` | oneBranch, onePull |
| `nonagenticai/conveyor#203` | verifyParts |
| `nonagenticai/conveyor#205` | verifyParts |
| `nonagenticai/conveyor#212` | oneBranch, onePull, verifyParts |
| `nonagenticai/conveyor#213` | verifyParts |
| `nonagenticai/conveyor#226` | verifyParts |
| `nonagenticai/conveyor#274` | verifyParts |
| `nonagenticai/conveyor#299` | verifyParts |
| `nonagenticai/conveyor#307` | verifyParts |
| `nonagenticai/conveyor#311` | verifyParts |
| `nonagenticai/conveyor#319` | oneBranch, onePull |
| `nonagenticai/conveyor#389` | verifyParts |
| `nonagenticai/conveyor#390` | verifyParts |
| `nonagenticai/conveyor#416` | verifyParts |
| `nonagenticai/conveyor#434` | verifyParts |
| `nonagenticai/conveyor#45` | verifyParts |
| `nonagenticai/conveyor#471` | verifyParts |
| `nonagenticai/conveyor#542` | verifyParts |
| `nonagenticai/conveyor#545` | verifyParts |
| `nonagenticai/conveyor#572` | verifyParts |
| `nonagenticai/conveyor#59` | verifyParts |
| `nonagenticai/conveyor#62` | verifyParts |
| `nonagenticai/conveyor#653` | verifyParts |
| `nonagenticai/conveyor#667` | verifyParts |
| `nonagenticai/conveyor#669` | mergeComplete |
| `nonagenticai/conveyor#73` | verifyParts |
| `nonagenticai/conveyor#797` | verifyParts |
| `nonagenticai/conveyor#81` | oneBranch, onePull |
| `nonagenticai/conveyor#92` | verifyParts |
| `nonagenticai/conveyor#pr-493` | closesKeyword, mergeComplete, verifyParts |
| `nonagenticai/conveyor#pr-495` | closesKeyword, mergeComplete, verifyParts |
| `nonagenticai/conveyor#pr-578` | closesKeyword, mergeComplete, verifyParts |

Rules the snapshot cannot reconstruct are named, not dropped: claim timing/comments (the four pre-branch checks are not a property of the finished unit) and `Blocker:` iff `status:blocked` (label state is not historical).

**`Verify:` cross-check.** The fact reuses `scripts/ci/board.py`'s `VERIFY_RE` (the line rule `issue-dor` runs) and requires one per `###` part, which is what 03-DELIVERY's DoR says. The checker now enforces the same per-part rule, so a disagreement here means a pre-2026-11-18 body or a heading the model accepts loosely (for example `## Parts`). Disagreements: 0 pass the checker but fail the model, 6 the reverse, 336 agree. Both answers are recorded; neither is silently adopted.

The kernel certifies *facts ⇒ verdict*; the collector's snapshot, rule ids and artefact digests are what a reviewer checks against GitHub. The snapshot is redacted per `scripts/ci/no-private-refs.sh` (public GitHub quotations of estate identifiers become placeholders; the facts need none of them). No unit is attributed to a person.
