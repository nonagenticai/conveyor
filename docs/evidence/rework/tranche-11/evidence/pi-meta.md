# PI metadata — tranche 11

Action: `cadences.update` on `01a222e7-0eb9-7386-8e46-4b8f39fb8302` with
`docs/evidence/record/inputs/tr11/cadence_tranche-11_pi_meta.json`, `--if-match 1`. The read-back is
`cadences.list` with `--read-input '{"programme_id":"01a1d039-0d24-7d16-ae52-4841e96ccfac"}'`,
because `cadences.list` requires `programme_id` and the update's own input does not carry it.

The row now carries `meta.pi_plan` — scope S14/T6/T8, period `tranche-11`, capacity, WIP limits,
four objectives, nine dependency edges and the refinement cadence id — and `meta.documents` with
`pi-plan` `01a222e7-31e3-738a-a4a6-854970f4bff2` and `pi-objectives` `01a222e7-42de-739b-9381-6f5743287704`. Version 2.
