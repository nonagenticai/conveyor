# Definition of Ready — T3 PM job

## Independent

The PM request-to-plan path ships alone: `pm.plan`, the four PM gates, the change path and the
closure. Project creation and the board surfaces (S05) are a declared boundary, not a
dependency.

## Negotiable

The scope is SPEC_V2 §12 T3's three clauses; the implementation shape of each gate is the
PM job's, and the manifest — not this document — is the gate.

## Valuable

A governed project lifecycle: a request becomes a planned, baselined project; every gate is
dual-controlled; a change travels as a change request; a project closes only on its record.

## Estimable

Bounded by `acceptance/programme/T3.toml`'s checks (project, baseline, charter, closure outputs,
contract, review, closure gate, waivers, PI cadence/plan/objectives, WSJF, demo credits,
lessons, product, artefact standard, rework).

## Small

One project, one path, one manifest; the programme surfaces and the slice's UI are out of scope.

## Testable

Every clause names its proof: the manifest checks, `crates/actions/tests/spine_gates.rs`,
`programme_baseline.rs`, `closure_gate.rs`, `pm_gate_proof.rs`.

## Dependencies

issue #74; tranche 3 authorised; the delivery contract and its Phase-G review (this unit); the
frozen scope baseline `01a2052e-061f-7570-b780-d94101449bef`, the schedule and cost baselines,
and the charter gate. The B3 benefit profile's T6 dependency is recorded on the programme
benefit evaluation — it is not T3's.
