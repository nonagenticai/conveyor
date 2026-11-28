# Evidence — tranche-9 PI outputs carry the period in `unit`

Measured 2026-11-28 on the replay record, snapshot `/tmp/snap-9`, after the two `documents.compose`
writes and the `outputs.js` re-read.

| output | id | project | category | unit | published_at |
|---|---|---|---|---|---|
| PI plan | `01a222b4-5dee-7060-a889-af69cf6d2ddf` | P0 `01a1dcb8-d24b-76de-b901-56fd56e001bc` | `pi-plan` | `tranche-9` | `2026-11-28T15:09:35.588792Z` |
| PI objectives | `01a222b4-5eb7-777f-b317-c0e445e762d7` | P0 `01a1dcb8-d24b-76de-b901-56fd56e001bc` | `pi-objectives` | `tranche-9` | `2026-11-28T15:09:35.788607Z` |

`DocumentComposeIn` carries `unit` (`crates/actions/src/documents/actions.rs`), and
`verify-project.py::_published_for_period` reads the literal token `tranche-9` from title or unit,
so composing the period into `unit` at creation is what makes every member's `pi_plan` and
`pi_objectives` check see these rows. The measured tranche-7 blocker — a published row with
`unit: null` that the period check cannot see — does not apply here: both rows read back with the
period set, and the snapshot's `outputs.js` returns both under `unit: "tranche-9"`.

The plan gate is separate from the row state: `cadences.approve_plan` wrote approval
`01a222b4-f15c-7c4e-ae37-153f5bfc3e80` with gate `cadences.approve_plan`, status `approved`, and a
`rule` decision by `01a1cf1e-09dd-7580-99cc-de395f3e86cc` under the standing delegation
`01a1ff1f-e4ba-72dc-9c1a-13049933cf14` — the same shape as tranche 8's
`01a2229e-bdf3-74ca-bf5f-17f28543c095`. The `pi` cadence row's own state stays
`awaiting_approval`; the approval row is the gate, and the snapshot's `msp.json` carries the plan's
metadata on the row.
