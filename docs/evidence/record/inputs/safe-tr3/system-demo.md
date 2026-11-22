# System Demo — conveyor buildout, tranche 3

The integrated increment demonstrated at the end of tranche 3, what was credited, and what the
demo could not show.

## Demonstrated

The T13 tool platform deliverable (`01a20277-8d10-702f-a975-4dd6469e7b1d`, status `accepted`):
one governed call path with the release grant enforced at `tools.call`, job storage over the
broker with tenant-prefixed keys, retirement over the zero-use report, and the long-op resume
path (pending + resume_token) as it lands. The call ledger records each call and its typed
error classes.

## Evidence

The accepted deliverable carries three typed evidence refs. The supporting records are the broker
tests, the accepted WBS row and the lane commits for `v1-p111-2`, `v1-p1110-1` and `v1-p119-1`.

## Credits

One credit, `01a202c3-3e75-7861-94f3-d4773631bc04`, recorded at the tr3 demo cadence
`01a20280-ef3f-7aac-8fad-3840a7995847` for deliverable `01a20277-8d10-702f-a975-4dd6469e7b1d`.
The credit copies the deliverable's evidence at credit time (3 refs). No demo, no credit: the
credit exists only because the deliverable is accepted with typed evidence.

## Findings

- `v1-w4a-5` (long-op resume) is the remaining T13 P1 and is named in the plan as committed.
- `v1-p119-1`'s schema-drift kata half is re-anchored to T5: a destination 422 has no instrument,
  and T5 must add a contract-test instrument class before a kata can open.

## Links

- Deliverable: `01a20277-8d10-702f-a975-4dd6469e7b1d`; cadence: `01a20280-ef3f-7aac-8fad-3840a7995847`.
- Tranche: `01a1e9e0-eb54-74d8-a2b5-44fb2b87eb52`.
