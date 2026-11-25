# T19F — Evidence

What was claimed, what was measured, and what remains a gap.

## Claim

The T19F forum bindings are delivered on `main` (`eb0deb61`) and proven on the deployed engine: a
meeting stays pinned to forum-policy v1 while the v2 route is exercised, a board subject is decided
in a sitting whose per-seat decision row carries `via.surface='meeting'`, and a cadence row
materialises exactly one meeting. The policy v2 itself awaits one human review decision, which the
product's authoring gate refuses to let a machine supply.

## Evidence

- Construction: PR #700 merged as `eb0deb61`; migration 0073 applied on the live database
  (`_sqlx_migrations` max 73); the deployed image carries it (pod digest `9c80514d…`).
- Programme row: project `T19F` `01a210ba-b6cb-76cc-82a3-a2b45cfb498f` on its board, `programmes.refresh`
  reports `created 0, skipped 52`.
- Governance chain: `scripts/governance-chain.py --project T19F --check` passes 24/24 on the
  published snapshot — seven approved artefacts, seven links, a live Statement of Architecture Work
  and a live phase-F delivery contract (requester ≠ approver), B7's baseline read back and not
  overwritten, one Phase-G compliance review per contract (level `conformant`), and the published
  `architecture-artifact` document with its Metadata/Entries/Links/Document headings.
- Live gate: `scripts/t19f-live-gate-check.py --check` passes 11 of 12 invariants:
  - the meeting `01a210c2-a327-7dc7-bab4-5a402a8497a8` is pinned at policy `t19f_live_gate_forum`
    version 1; the pinned meeting still resolves v1 after the v2 route was exercised;
  - the sitting body `01a210c8-579b-7808-bc32-aef8899b0b7a` decided contract
    `01a210c8-7ea8-7dc9-8104-700ecfe2905b` in meeting `01a210c8-94ee-73ba-9147-88f2b4729ae0`;
    approval `01a210c8-9273-7964-9b97-0f3dd810bcf3` is `approved` with exactly one decision row
    carrying `via.surface='meeting'` and naming the chair (`conveyor-approver`); the agenda item
    closed as `decision`;
  - cadence `01a210c9-0d47-7473-8339-7ea7648e59e9` materialised twice to the same meeting
    `01a210c9-2397-73f1-8d07-7e8e807e524f`, carrying the bound forum type and the slot and not
    cancelled.

## Verification

```
python3 scripts/governance-chain.py --project T19F --check      # 24/24 PASS
python3 scripts/t19f-live-gate-check.py --check                  # 11/12 PASS, 1 blocked
```

The transports are `scripts/programme-record.py` (the deployed action surface) and, for the
snapshot, `docs/design/scripts/publish-data.sh`. Raw read-backs and acts are under
`docs/evidence/t19f/` (`gate/`, `gov/`, `live-gate.json`, `live-gate-check.txt`,
`publish-report-only.log`).

## Links

- Plan: `docs/plans/T19F-meeting-bindings.md`; artefacts under `docs/plans/T19F/`.
- Implementation: PR #700 (`Closes #683`), merge `eb0deb61`.
- This unit: issue #684.

## Gap

- **The policy v2 publish is blocked on one human review.** `authoring.review` requires the
  approval's last decision to be `human`; the admin CLI records `service` by design, so the gate
  refuses it verbatim (`BLOCKED policy v2: …`). A person must approve the review gate in the
  console, after which re-running `--apply` publishes v2 and the twelfth invariant passes. The
  asset `01a210c3-7c34-7338-89f2-6c7a6e40bea7` and its approved gate
  `01a210c3-9007-75f4-927c-d1dc32f54c6b` are in place for that click.
- The strict conformance publish is red on programme-wide debt that predates this unit (T5, T6,
  T8, T15 and T17 closed since the accepted baseline; their new/worsened P0s and the S18/S21/
  SPEC-V2 ledger rows). The T19F finding is `project_in_unauthorised_tranche` P1 — T19F is active
  in tranche 6, whose authorisation is #526's unit; the plan requires T19F to stay active until
  the tranche-6 review, so this is the declared state, not a regression. The pack was therefore
  published report-only with the exact findings in `publish-report-only.log`.
