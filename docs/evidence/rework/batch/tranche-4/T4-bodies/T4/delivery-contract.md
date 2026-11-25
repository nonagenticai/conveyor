# Delivery Contract — Kanban coach

## Commitments

The record the section derives from, or an explicit absence.

_Explicit absence — the contract's commitment list: a commitments field on the delivery contract body._

## Compliance method

The record the section derives from, or an explicit absence.

```json
{
  "category": "delivery-contract",
  "count": 1,
  "rows": [
    {
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "contract_id": "01a2028d-942b-759a-ad59-ab9c38629977",
      "field": "compliance",
      "kind": "contract_field",
      "title": "compliance: T4 is assessed clause by clause against the SPEC_V2 section 12 gate. Clause 1 ('an injected failure yields exactly one coach card and one remedy') is proved for a first interruption by `crates/actions/tests/coach.rs::an_interruption_yields_one_coach_card_one_triage_and_one_remedy` (line 577) and by the seeded-process test (line 1131), but is refuted as a universal by v1-p6-4: the per-card cap counts every interruption regardless of signature and refuses a fourth failed run with no coach card and no escalation, and `err.card` suppresses coaching when a live root run exists. Clause 2 ('a repeat signature escalates') is proved by `::a_repeat_signature_escalates_instead_of_a_second_card` (line 683). Clause 3 ('no coach card about a coach card') is proved by `::a_failed_coach_run_never_opens_a_coach_card` (line 745) and `::a_nested_coach_run_is_refused` (line 1343). Clause 4 ('a systemic class opens a kata') is proved only as a `kata.open` process start by `::a_systemic_class_opens_one_kata` (line 816), with `::a_transient_class_opens_no_kata` (line 864), and v1-p6-2 shows the path stores a run id, not a kata with instrument and threshold. The batch admitted four findings \u2014 v1-p6-2, v1-p6-4 and v1-p6-5 (P1) and v1-p6-6 (P2). The compliance level is non_conformant; no finding is P0, so the manifest's product gate does not reopen the register's `closed` status. The rework's acceptance instrument is `acceptance/programme/T4.toml`.",
      "value": "T4 is assessed clause by clause against the SPEC_V2 section 12 gate. Clause 1 ('an injected failure yields exactly one coach card and one remedy') is proved for a first interruption by `crates/actions/tests/coach.rs::an_interruption_yields_one_coach_card_one_triage_and_one_remedy` (line 577) and by the seeded-process test (line 1131), but is refuted as a universal by v1-p6-4: the per-card cap counts every interruption regardless of signature and refuses a fourth failed run with no coach card and no escalation, and `err.card` suppresses coaching when a live root run exists. Clause 2 ('a repeat signature escalates') is proved by `::a_repeat_signature_escalates_instead_of_a_second_card` (line 683). Clause 3 ('no coach card about a coach card') is proved by `::a_failed_coach_run_never_opens_a_coach_card` (line 745) and `::a_nested_coach_run_is_refused` (line 1343). Clause 4 ('a systemic class opens a kata') is proved only as a `kata.open` process start by `::a_systemic_class_opens_one_kata` (line 816), with `::a_transient_class_opens_no_kata` (line 864), and v1-p6-2 shows the path stores a run id, not a kata with instrument and threshold. The batch admitted four findings \u2014 v1-p6-2, v1-p6-4 and v1-p6-5 (P1) and v1-p6-6 (P2). The compliance level is non_conformant; no finding is P0, so the manifest's product gate does not reopen the register's `closed` status. The rework's acceptance instrument is `acceptance/programme/T4.toml`."
    }
  ],
  "source": "computed:contract_field"
}
```

## Approval

The record the section derives from, or an explicit absence.

```json
{
  "category": "delivery-contract",
  "count": 1,
  "rows": [
    {
      "approved_at": "2026-11-22T09:19:34.155323+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "id": "01a2028d-abf9-78fb-a9c6-eb06c4bdd634",
      "kind": "contract_approval",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "approved",
      "title": "approval 01a2028d-abf9-78fb-a9c6-eb06c4bdd634 \u2014 approved"
    }
  ],
  "source": "computed:contract_approval"
}
```
