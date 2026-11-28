# Evidence — tranche-8 PI plan approval

Opens and approves the PI plan's gate (`cadences.approve_plan`) against the `pi` cadence row. The decision is recorded as a `rule` under the published delegation 01a1ff1f-e4ba-72dc-9c1a-13049933cf14 — the mechanism the action documents for a delegated decision, not a person's signature. `materialise_pi_tx` refuses to draft cards until this gate carries an approve.
