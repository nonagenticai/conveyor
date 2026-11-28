# PI plan approval — tranche 11

Action: `cadences.approve_plan` on the PI cadence with
`docs/evidence/record/inputs/tr11/pi-plan_tranche-11.json`; read back with
`--read-action cadences.list --read-match kind=pi`.

- approval `01a222e7-9198-7400-8633-d3542cb68347`, decision `approve`, `decided_by_kind` `rule`, status `approved`, under the
  standing delegation `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`.
- the PI row's own state stays `awaiting_approval`, as tranche 8's and tranche 9's do: the gate is
  the approval row, not the cadence state.

The approval is the delegated rule decision the readiness session is authorised to make; the
tranche review that closes tranche 11 remains #755's human act.
