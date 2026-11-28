# Inspect and Adapt — conveyor buildout, tranche 3

The tranche-3 retro: what went well, what did not, the lessons recorded on the retro cadence,
the actions they produce and the kata candidates they raise.

## What went well

- The three registers are operated, not merely modelled: fourteen risks with owners and review
  dates, fifty-seven ADR rows seeded through the decisions actions, and B1–B7 baselined with
  B1/B2/B4/B5/B6 active under approved changes.
- The clean-build discipline produced a trustworthy binary and lock at `9ac63b41` (548 actions,
  579 operations, `--check-lock` OK) after the shared target had polluted an earlier build.
- The delegated rule path worked on every programme gate it was used on: PI plan, tranche
  review, change approvals, all recorded `rule` with a published `via.delegation_ref`.

## What did not

- The promotion journal lost its last row when a trim by line count ignored a header object; the
  `--check` grammar caught it, but only after the fact.
- The first lock was generated from a binary built against a stale shared target and had to be
  regenerated from a clean build.
- A benefit baseline edit resets the profile to `draft`; activation is a second recorded write,
  which cost an extra version on every profile.
- The schema-drift kata cannot open: a destination 422 has no eval/benchmark instrument, and T5
  refuses an unmeasurable kata.

## Lessons

- **k1.** A clean build before a lock regeneration is the only trustworthy verdict.
- **k2.** The promotion journal must be validated after every append.
- **k3.** A delegation document's owner project is part of its contract; the guard must accept
  every owner the delegation actually uses.
- **k4.** A measurement change resets a benefit profile to draft; activation is its own write.
- **k5.** A defect with no instrument cannot open a kata; re-anchor rather than fabricate.

## Actions

- `v1-w4a-5` proceeds as a deliberate slice (timeout from `timeout_secs`, pending + resume_token,
  a status tool).
- `promote-record.py` gains `decisions.create`/`decisions.transition` replay before the
  deploy-bound promotion.
- The three programme screens (Risk, Decisions, Benefits evaluation) and the per-project factory
  cards follow in tranche 4.

## Kata candidates

The schema-drift kata is a candidate under T5, contingent on the contract-test instrument class.
No kata row is opened until that instrument exists; the finding is re-anchored to T5 and stays
open.

## Links

- Retro cadence: `01a20281-09ba-709a-b61e-51a22f2ed172` (closed; lessons k1–k5 on the record).
- Refinement cadence: `01a20280-d52b-7791-a9b1-ebc2a10b5c3d`.
