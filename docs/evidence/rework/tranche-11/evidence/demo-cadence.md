# Demo cadence — tranche 11

Action: `cadences.create` with `docs/evidence/record/inputs/tr11/cadence_tranche-11_demo.json`.
Read back (`cadences.list`, `kind=demo`, `period=tranche-11`).

- id `01a222e7-1c46-7ffe-94d3-983ede8ad40e`, period `tranche-11`, state `planned`.

The batch passes this row to the factory as `--demo-cadence`, so each member's accepted deliverable
can be credited to a demo. Without the row the factory has no demo to credit and the receipt's
`demo-credits` check has nothing to read.
