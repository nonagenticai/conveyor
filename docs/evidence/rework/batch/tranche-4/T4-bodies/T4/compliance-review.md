# Compliance Review — Kanban coach

## Relationship

Exactly one of irrelevant, consistent, compliant, conformant, fully conformant or non-conformant.

```json
{
  "category": "compliance-review",
  "count": 1,
  "rows": [
    {
      "contract_id": "01a2028d-942b-759a-ad59-ab9c38629977",
      "created_at": "2026-11-22T09:20:30.699998+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/T4.toml",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "manifest": "acceptance/programme/T4.toml",
        "sha": "fc4ed551b7ad1a1c062781d6b820d0d700633c86"
      },
      "findings": "This retrospective Phase-G review records the compliance level non_conformant for T4 under the standing delegation `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`. It names the four findings this batch admitted, each by id and by what is missing. v1-p6-2 (P1) \u2014 the systemic path terminates at a process-run id: `coach.rs:994-999` starts `KATA_PROCESS` with `{interruption_id}` and stores the run id as `kata_candidate_id`; there is no kata record naming an instrument and success threshold, no tenant-kata scoping, no measurement and no adoption gate, so a systemic finding never changes production. v1-p6-4 (P1) \u2014 interruptions are silently dropped: the cap counts every interruption on a card regardless of signature (`count_interruptions_for_card_tx`, line 603), refuses the fourth failed run at `COACH_INTERRUPTION_MAX` (line 608) with no coach card and no escalation, and `board/pull.rs:359` suppresses coaching whenever any live root run exists on the card. v1-p6-5 (P1) \u2014 the coach is wired only to failed runs: `coach.interrupt` has one caller, `err.card` (`board/pull.rs:424`); WIP breaches, stale leases, gate timeouts and board events, all section 6.2 inputs, have no producer. v1-p6-6 (P2) \u2014 orphan coach cards can be created by the public create path: `board/cards.rs:104` accepts any catalogue card type, including coach, and validates neither an interruption row nor `meta.interrupted_card_id`. Clause dispositions. Clause 2 is proved by `coach.rs::a_repeat_signature_escalates_instead_of_a_second_card` (line 683) and clause 3 by `::a_failed_coach_run_never_opens_a_coach_card` (line 745) with `::a_nested_coach_run_is_refused` (line 1343). Clause 1 has a named passing test for a first interruption (`::an_interruption_yields_one_coach_card_one_triage_and_one_remedy`, line 577) but is refuted as a universal by v1-p6-4, so it is not proved at HEAD. Clause 4 has a named passing test (`::a_systemic_class_opens_one_kata`, line 816, with `::a_transient_class_opens_no_kata`, line 864) but proves only a process start; v1-p6-2 shows the kata record, instrument and threshold that the clause intends are absent, so the clause is proved only in the weaker sense. The close also carried a mandatory_artefacts gap: at the 2026-11-22 rework snapshot the seven Phase A\u2013D artefacts, the Statement of Architecture Work and the delivery contract were absent; this rework produces and publishes stakeholder_map, driver_goal_objective, requirements_catalog and solution_concept (Phase A), business_capability_map (Phase B), data_entity_catalog (Phase C) and technology_standards (Phase D), and records this review. No original Phase-G review existed; this one is retrospective and does not attest contemporaneous intent. The register's `closed` status is retained because no open finding is P0 and the manifest's product gate is P0-only; v1-p6-2, v1-p6-4 and v1-p6-5 are carried as P1 packages and v1-p6-6 as P2, none waived.",
      "id": "01a2028e-a1be-779d-a762-8d118fc2ec0b",
      "level": "non_conformant",
      "outcome": null,
      "project_id": "01a1d039-0d33-7e7e-890e-b437b0c78db9",
      "reviewed_at": "2026-11-22T09:20:30.699998+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-22T09:20:30.699998+00:00",
      "version": 1
    }
  ],
  "source": "architecture.json#/compliance_reviews"
}
```

## Findings

The evidence for each finding, against the contract.

```json
{
  "category": "compliance-review",
  "count": 1,
  "rows": [
    {
      "contract_id": "01a2028d-942b-759a-ad59-ab9c38629977",
      "created_at": "2026-11-22T09:20:30.699998+00:00",
      "evidence": {
        "acceptance_manifest": "acceptance/programme/T4.toml",
        "delegation_ref": "01a1ff1f-e4ba-72dc-9c1a-13049933cf14",
        "manifest": "acceptance/programme/T4.toml",
        "sha": "fc4ed551b7ad1a1c062781d6b820d0d700633c86"
      },
      "findings": "This retrospective Phase-G review records the compliance level non_conformant for T4 under the standing delegation `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`. It names the four findings this batch admitted, each by id and by what is missing. v1-p6-2 (P1) \u2014 the systemic path terminates at a process-run id: `coach.rs:994-999` starts `KATA_PROCESS` with `{interruption_id}` and stores the run id as `kata_candidate_id`; there is no kata record naming an instrument and success threshold, no tenant-kata scoping, no measurement and no adoption gate, so a systemic finding never changes production. v1-p6-4 (P1) \u2014 interruptions are silently dropped: the cap counts every interruption on a card regardless of signature (`count_interruptions_for_card_tx`, line 603), refuses the fourth failed run at `COACH_INTERRUPTION_MAX` (line 608) with no coach card and no escalation, and `board/pull.rs:359` suppresses coaching whenever any live root run exists on the card. v1-p6-5 (P1) \u2014 the coach is wired only to failed runs: `coach.interrupt` has one caller, `err.card` (`board/pull.rs:424`); WIP breaches, stale leases, gate timeouts and board events, all section 6.2 inputs, have no producer. v1-p6-6 (P2) \u2014 orphan coach cards can be created by the public create path: `board/cards.rs:104` accepts any catalogue card type, including coach, and validates neither an interruption row nor `meta.interrupted_card_id`. Clause dispositions. Clause 2 is proved by `coach.rs::a_repeat_signature_escalates_instead_of_a_second_card` (line 683) and clause 3 by `::a_failed_coach_run_never_opens_a_coach_card` (line 745) with `::a_nested_coach_run_is_refused` (line 1343). Clause 1 has a named passing test for a first interruption (`::an_interruption_yields_one_coach_card_one_triage_and_one_remedy`, line 577) but is refuted as a universal by v1-p6-4, so it is not proved at HEAD. Clause 4 has a named passing test (`::a_systemic_class_opens_one_kata`, line 816, with `::a_transient_class_opens_no_kata`, line 864) but proves only a process start; v1-p6-2 shows the kata record, instrument and threshold that the clause intends are absent, so the clause is proved only in the weaker sense. The close also carried a mandatory_artefacts gap: at the 2026-11-22 rework snapshot the seven Phase A\u2013D artefacts, the Statement of Architecture Work and the delivery contract were absent; this rework produces and publishes stakeholder_map, driver_goal_objective, requirements_catalog and solution_concept (Phase A), business_capability_map (Phase B), data_entity_catalog (Phase C) and technology_standards (Phase D), and records this review. No original Phase-G review existed; this one is retrospective and does not attest contemporaneous intent. The register's `closed` status is retained because no open finding is P0 and the manifest's product gate is P0-only; v1-p6-2, v1-p6-4 and v1-p6-5 are carried as P1 packages and v1-p6-6 as P2, none waived.",
      "id": "01a2028e-a1be-779d-a762-8d118fc2ec0b",
      "level": "non_conformant",
      "outcome": null,
      "project_id": "01a1d039-0d33-7e7e-890e-b437b0c78db9",
      "reviewed_at": "2026-11-22T09:20:30.699998+00:00",
      "reviewed_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "reviewed_by_name": "conveyor-approver",
      "updated_at": "2026-11-22T09:20:30.699998+00:00",
      "version": 1
    }
  ],
  "source": "architecture.json#/compliance_reviews"
}
```

## Waivers

Each accepted deviation with its owner, reason, cost, risk and reassessment date.

```json
{
  "category": "compliance-review",
  "count": 1,
  "rows": [
    {
      "kind": "waiver_status",
      "title": "None recorded \u2014 0 waivers for this project",
      "waivers": 0
    }
  ],
  "source": "computed:waiver_status"
}
```

## Decision

The outcome and the conditions that attach to it.

```json
{
  "category": "compliance-review",
  "count": 1,
  "rows": [
    {
      "conditions": null,
      "findings": "This retrospective Phase-G review records the compliance level non_conformant for T4 under the standing delegation `01a1ff1f-e4ba-72dc-9c1a-13049933cf14`. It names the four findings this batch admitted, each by id and by what is missing. v1-p6-2 (P1) \u2014 the systemic path terminates at a process-run id: `coach.rs:994-999` starts `KATA_PROCESS` with `{interruption_id}` and stores the run id as `kata_candidate_id`; there is no kata record naming an instrument and success threshold, no tenant-kata scoping, no measurement and no adoption gate, so a systemic finding never changes production. v1-p6-4 (P1) \u2014 interruptions are silently dropped: the cap counts every interruption on a card regardless of signature (`count_interruptions_for_card_tx`, line 603), refuses the fourth failed run at `COACH_INTERRUPTION_MAX` (line 608) with no coach card and no escalation, and `board/pull.rs:359` suppresses coaching whenever any live root run exists on the card. v1-p6-5 (P1) \u2014 the coach is wired only to failed runs: `coach.interrupt` has one caller, `err.card` (`board/pull.rs:424`); WIP breaches, stale leases, gate timeouts and board events, all section 6.2 inputs, have no producer. v1-p6-6 (P2) \u2014 orphan coach cards can be created by the public create path: `board/cards.rs:104` accepts any catalogue card type, including coach, and validates neither an interruption row nor `meta.interrupted_card_id`. Clause dispositions. Clause 2 is proved by `coach.rs::a_repeat_signature_escalates_instead_of_a_second_card` (line 683) and clause 3 by `::a_failed_coach_run_never_opens_a_coach_card` (line 745) with `::a_nested_coach_run_is_refused` (line 1343). Clause 1 has a named passing test for a first interruption (`::an_interruption_yields_one_coach_card_one_triage_and_one_remedy`, line 577) but is refuted as a universal by v1-p6-4, so it is not proved at HEAD. Clause 4 has a named passing test (`::a_systemic_class_opens_one_kata`, line 816, with `::a_transient_class_opens_no_kata`, line 864) but proves only a process start; v1-p6-2 shows the kata record, instrument and threshold that the clause intends are absent, so the clause is proved only in the weaker sense. The close also carried a mandatory_artefacts gap: at the 2026-11-22 rework snapshot the seven Phase A\u2013D artefacts, the Statement of Architecture Work and the delivery contract were absent; this rework produces and publishes stakeholder_map, driver_goal_objective, requirements_catalog and solution_concept (Phase A), business_capability_map (Phase B), data_entity_catalog (Phase C) and technology_standards (Phase D), and records this review. No original Phase-G review existed; this one is retrospective and does not attest contemporaneous intent. The register's `closed` status is retained because no open finding is P0 and the manifest's product gate is P0-only; v1-p6-2, v1-p6-4 and v1-p6-5 are carried as P1 packages and v1-p6-6 as P2, none waived.",
      "id": "01a2028e-a1be-779d-a762-8d118fc2ec0b",
      "kind": "governing_review",
      "level": "non_conformant",
      "outcome": null,
      "reviewed_at": "2026-11-22T09:20:30.699998+00:00",
      "reviewed_by_name": "conveyor-approver",
      "title": "governing review 01a2028e-a1be-779d-a762-8d118fc2ec0b: non_conformant / no outcome"
    }
  ],
  "source": "computed:governing_review"
}
```
