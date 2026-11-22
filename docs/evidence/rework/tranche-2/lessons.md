# Tranche 2 — the retro lessons, one per finding

**Recorded 2026-11-22** against the tranche-2 retro cadence `01a2004a-9c9a-7999-bbbd-9976427009b6`
under programme `01a1d039-0d24-7d16-ae52-4841e96ccfac`. Every lesson names the finding it comes
from and the evidence that settles it — a resolution commit and its test where the finding is
fixed, the open defect where it is not. The standard is one lesson per finding; this batch admitted
seven, so it records seven.

| Finding | Severity | State at the lesson | Lesson row |
|---|---|---|---|
| `v1-t2-1` | P0 | fixed `920a1dcb`, `documents_compose_gate.rs` | `01a20094-12b4-7584-ac12-4ff51e5324e9` |
| `v1-t2-3` | P1 | fixed `fcb96fa9`, `decision_ladder.rs` | `01a20094-3005-7ec4-8eb0-75cf61bdd6ba` |
| `v1-t2-5` | P2 | **open** — refusal code still `tool_budget_exceeded` | `01a20094-50ba-790c-a5ab-8d70f7d4f8b0` |
| `v1-t2-6` | P1 | fixed `1167127d`, `ladder_human_gate.rs` | `01a20094-6c59-781c-9a0e-4181f771e284` |
| `v1-s03-1` | P1 | fixed `beaa0404`, `claim_race.rs` | `01a20094-8c61-7174-82aa-bdd6f0816a62` |
| `v1-s03-2` | P2 | **open** — two card indexes do not filter archived rows | `01a20094-aa0b-7677-ad0d-dfc0f5b5ef9b` |
| `v1-tr2-1` | P1 | fixed `aad18225`, `spine_gates.rs` | `01a20094-ca8b-743e-a9fe-8a97b195c587` |

## The lessons

**`v1-t2-1` — a gated artefact has more than one door.** `documents.update` refused to attach a
document whose caller-supplied `meta.generated` was true, but `documents.compose` wrote `card_id` at
creation without consulting it, so a model's output could reach a card with no ladder decision. The
lesson is that the gate is a property of *every* write path that can attach a generated artefact,
not of the one path the spec's sentence happens to name.

**`v1-t2-3` — publishing a refinement is a policy change.** A refinement that changes a threshold
could be published without the gate that a policy change requires. The lesson is that "publish" is
not a neutral act when the object it publishes is a rule: the gate must sit on the publication, not
only on the object's creation.

**`v1-t2-5` — a refusal code is part of the contract.** The budget refusal returned
`tool_budget_exceeded` where the spec names `budget_exceeded`, and no test caught it because the
tests asserted the code the implementation produced. The lesson is that a caller-visible error code
is a requirement, and a test written from the implementation cannot prove it. *(Open: the finding is
still unresolved; this lesson records the defect, not a fix.)*

**`v1-t2-6` — a human decision must cite the gate that posed its question.** `ladder.record`
accepted any approved approval, not the one whose gate asked the question, so a human signature could
be re-used across gates. The lesson is that an approval is a proof of a specific question being
answered; binding it to its target is what makes the signature mean anything.

**`v1-s03-1` — a property the code does not hold is not a property.** The claim-exclusivity property
was stated in the gate sentence while the code permitted repeat claims, and the race test never
exercised the repeat path. The lesson is that a stated property needs a test that fails when the
property is removed — a test that only exercises the happy path proves nothing about exclusivity.

**`v1-s03-2` — an "every" claim needs a census, not an example.** The archive-exclusion claim said
every hot index filters archived cards; two card indexes do not. The lesson is that a universally
quantified claim is verified by enumerating the set, not by inspecting a member. *(Open: the two
indexes still do not filter.)*

**`v1-tr2-1` — a pause is a one-way door.** Pausing a tranche moves it to `paused`, a tranche is
reviewed once, and `projects.close` accepts only `authorised | in_review | closed` — so the pause
froze the two projects it was meant to gate, and nothing could reopen it. `tranches.resume` now
exists and needs a human-approved gate, which is the right price; the lesson is that a state
machine's terminal states must be walked before they are used, and that an exit-criteria failure
must name its path back in the review itself.

## Why this file exists

The tranche-2 review was recorded as `pause` with no lessons, and the project manifests' `lessons`
check reads exactly this: at least one lesson whose `cadence_id` is the tranche's retro. The gap was
found by the full-fidelity snapshot loop (`docs/evidence/rework/full-fidelity-snapshot.md`), not by
prose review — the deployed-engine snapshot could not see the retro at all.
