# PI cadence — tranche 11

Action: `cadences.create` with `docs/evidence/record/inputs/tr11/cadence_tranche-11_pi.json`.
Read back on the replay record (`cadences.list`, `--read-match kind=pi --read-match period=tranche-11`).

- id `01a222e7-0eb9-7386-8e46-4b8f39fb8302`, kind `pi`, period `tranche-11`, state `planned` at creation; version 2 after the
  PI metadata update.
- meta at creation: `tranche_id` `01a1e9e1-33f4-70a7-8af4-658f1c583e99`, `tranche_number` 11,
  `delegation_ref` `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`.

```
python3 scripts/programme-record.py --action cadences.create \
  --input @docs/evidence/record/inputs/tr11/cadence_tranche-11_pi.json \
  --evidence docs/evidence/record/inputs/tr11/cadence_tranche-11_pi.json \
  --read-action cadences.list \
  --read-input '{"programme_id":"01a1d039-0d24-7d16-ae52-4841e96ccfac"}' \
  --read-match kind=pi --read-match period=tranche-11 \
  --local-bin /tmp/conv11-bin/conveyor --quiet
```

The row is what clears `pi_cadence_missing:tranche-11`: the audit at the pinned snapshot no longer
reports that rule for this tranche.
