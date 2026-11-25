# Statement Of Architecture Work — Kanban coach

## Assignment

What the architecture work is asked to deliver and for whom.

```json
{
  "category": "statement-of-architecture-work",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a2028b-f63c-7c27-b580-d4cba2c1e70f",
      "approved_at": "2026-11-22T09:17:42.179607+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 section 12 T4 gate: an injected failure yields exactly one coach card and one remedy; a repeat signature escalates; no coach card about a coach card; a systemic class opens a kata. Clauses 2 and 3 are proved by named tests in `crates/actions/tests/coach.rs`; clause 1 is proved for a first interruption but refuted as a universal by v1-p6-4; clause 4 is proved only as a `kata.open` process start and v1-p6-2 shows no kata record, instrument or threshold is created. The rework's acceptance instrument is `acceptance/programme/T4.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T4 (SPEC_V2 sections 6.2, 6.4 and 12, issue #75) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the section 12 gate, and publish the project's expected outputs (`project-brief`, `evidence`) and closure output (`closure-review`). The project is already `closed` in the register; this pass records the lifecycle retrospectively and does not reopen it.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the interruption taxonomy, the closed remedy set, per-signature uniqueness and the depth cap of SPEC_V2 section 6.2, plus the systemic-class path of section 6.4: the `coach_interruption` and `coach_remedy` tables, the `coach.interrupt` action, the seeded coach process and the real-Postgres suite `crates/actions/tests/coach.rs`. Out of scope: the board and pull path's `err.card` producer, the kata's own lifecycle and measurement (T5), the PM job (T3), operations (T7) and the tool platform (T13)."
      },
      "created_at": "2026-11-22T09:17:30.02765+00:00",
      "id": "01a2028b-dde6-7ed4-9e47-6e582f2341d8",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7e7e-890e-b437b0c78db9",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T4 Kanban coach (retrospective, U5a)",
      "updated_at": "2026-11-22T09:17:42.179607+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

## Scope

The boundaries, the artefacts expected and the phase set.

```json
{
  "category": "statement-of-architecture-work",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a2028b-f63c-7c27-b580-d4cba2c1e70f",
      "approved_at": "2026-11-22T09:17:42.179607+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 section 12 T4 gate: an injected failure yields exactly one coach card and one remedy; a repeat signature escalates; no coach card about a coach card; a systemic class opens a kata. Clauses 2 and 3 are proved by named tests in `crates/actions/tests/coach.rs`; clause 1 is proved for a first interruption but refuted as a universal by v1-p6-4; clause 4 is proved only as a `kata.open` process start and v1-p6-2 shows no kata record, instrument or threshold is created. The rework's acceptance instrument is `acceptance/programme/T4.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T4 (SPEC_V2 sections 6.2, 6.4 and 12, issue #75) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the section 12 gate, and publish the project's expected outputs (`project-brief`, `evidence`) and closure output (`closure-review`). The project is already `closed` in the register; this pass records the lifecycle retrospectively and does not reopen it.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the interruption taxonomy, the closed remedy set, per-signature uniqueness and the depth cap of SPEC_V2 section 6.2, plus the systemic-class path of section 6.4: the `coach_interruption` and `coach_remedy` tables, the `coach.interrupt` action, the seeded coach process and the real-Postgres suite `crates/actions/tests/coach.rs`. Out of scope: the board and pull path's `err.card` producer, the kata's own lifecycle and measurement (T5), the PM job (T3), operations (T7) and the tool platform (T13)."
      },
      "created_at": "2026-11-22T09:17:30.02765+00:00",
      "id": "01a2028b-dde6-7ed4-9e47-6e582f2341d8",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7e7e-890e-b437b0c78db9",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T4 Kanban coach (retrospective, U5a)",
      "updated_at": "2026-11-22T09:17:42.179607+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

## Responsibilities

Who does the work and who accepts it.

```json
{
  "category": "statement-of-architecture-work",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a2028b-f63c-7c27-b580-d4cba2c1e70f",
      "approved_at": "2026-11-22T09:17:42.179607+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 section 12 T4 gate: an injected failure yields exactly one coach card and one remedy; a repeat signature escalates; no coach card about a coach card; a systemic class opens a kata. Clauses 2 and 3 are proved by named tests in `crates/actions/tests/coach.rs`; clause 1 is proved for a first interruption but refuted as a universal by v1-p6-4; clause 4 is proved only as a `kata.open` process start and v1-p6-2 shows no kata record, instrument or threshold is created. The rework's acceptance instrument is `acceptance/programme/T4.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T4 (SPEC_V2 sections 6.2, 6.4 and 12, issue #75) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the section 12 gate, and publish the project's expected outputs (`project-brief`, `evidence`) and closure output (`closure-review`). The project is already `closed` in the register; this pass records the lifecycle retrospectively and does not reopen it.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the interruption taxonomy, the closed remedy set, per-signature uniqueness and the depth cap of SPEC_V2 section 6.2, plus the systemic-class path of section 6.4: the `coach_interruption` and `coach_remedy` tables, the `coach.interrupt` action, the seeded coach process and the real-Postgres suite `crates/actions/tests/coach.rs`. Out of scope: the board and pull path's `err.card` producer, the kata's own lifecycle and measurement (T5), the PM job (T3), operations (T7) and the tool platform (T13)."
      },
      "created_at": "2026-11-22T09:17:30.02765+00:00",
      "id": "01a2028b-dde6-7ed4-9e47-6e582f2341d8",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7e7e-890e-b437b0c78db9",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T4 Kanban coach (retrospective, U5a)",
      "updated_at": "2026-11-22T09:17:42.179607+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

## Acceptance criteria

The conditions the architecture must meet to close Phase A.

```json
{
  "category": "statement-of-architecture-work",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a2028b-f63c-7c27-b580-d4cba2c1e70f",
      "approved_at": "2026-11-22T09:17:42.179607+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 section 12 T4 gate: an injected failure yields exactly one coach card and one remedy; a repeat signature escalates; no coach card about a coach card; a systemic class opens a kata. Clauses 2 and 3 are proved by named tests in `crates/actions/tests/coach.rs`; clause 1 is proved for a first interruption but refuted as a universal by v1-p6-4; clause 4 is proved only as a `kata.open` process start and v1-p6-2 shows no kata record, instrument or threshold is created. The rework's acceptance instrument is `acceptance/programme/T4.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T4 (SPEC_V2 sections 6.2, 6.4 and 12, issue #75) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the section 12 gate, and publish the project's expected outputs (`project-brief`, `evidence`) and closure output (`closure-review`). The project is already `closed` in the register; this pass records the lifecycle retrospectively and does not reopen it.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the interruption taxonomy, the closed remedy set, per-signature uniqueness and the depth cap of SPEC_V2 section 6.2, plus the systemic-class path of section 6.4: the `coach_interruption` and `coach_remedy` tables, the `coach.interrupt` action, the seeded coach process and the real-Postgres suite `crates/actions/tests/coach.rs`. Out of scope: the board and pull path's `err.card` producer, the kata's own lifecycle and measurement (T5), the PM job (T3), operations (T7) and the tool platform (T13)."
      },
      "created_at": "2026-11-22T09:17:30.02765+00:00",
      "id": "01a2028b-dde6-7ed4-9e47-6e582f2341d8",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7e7e-890e-b437b0c78db9",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T4 Kanban coach (retrospective, U5a)",
      "updated_at": "2026-11-22T09:17:42.179607+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```

## Contract

The contract record, its version and its approval.

```json
{
  "category": "statement-of-architecture-work",
  "count": 1,
  "rows": [
    {
      "approval_id": "01a2028b-f63c-7c27-b580-d4cba2c1e70f",
      "approved_at": "2026-11-22T09:17:42.179607+00:00",
      "approved_by": "01a1e8a7-8b23-76a9-9114-aafb95f7a545",
      "body": {
        "acceptance_criteria": "The SPEC_V2 section 12 T4 gate: an injected failure yields exactly one coach card and one remedy; a repeat signature escalates; no coach card about a coach card; a systemic class opens a kata. Clauses 2 and 3 are proved by named tests in `crates/actions/tests/coach.rs`; clause 1 is proved for a first interruption but refuted as a universal by v1-p6-4; clause 4 is proved only as a `kata.open` process start and v1-p6-2 shows no kata record, instrument or threshold is created. The rework's acceptance instrument is `acceptance/programme/T4.toml`. The A\u2013D set and this contract are board-approved, and the Phase-G review cites the standing delegation and records the compliance level non_conformant.",
        "assignment": "Rework the V2 slice T4 (SPEC_V2 sections 6.2, 6.4 and 12, issue #75) by the reconstituted TOGAF/MSP lifecycle: produce and publish the seven mandatory Phase A\u2013D artefacts, the charter, this Statement of Architecture Work and the delivery contract, record the project's one WBS deliverable, take the Phase-G compliance review against the section 12 gate, and publish the project's expected outputs (`project-brief`, `evidence`) and closure output (`closure-review`). The project is already `closed` in the register; this pass records the lifecycle retrospectively and does not reopen it.",
        "responsibilities": {
          "approver": "conveyor-approver",
          "architecture board": "conveyor-architecture-board",
          "programme architect": "conveyor-e2e"
        },
        "retrospective": true,
        "scope": "the interruption taxonomy, the closed remedy set, per-signature uniqueness and the depth cap of SPEC_V2 section 6.2, plus the systemic-class path of section 6.4: the `coach_interruption` and `coach_remedy` tables, the `coach.interrupt` action, the seeded coach process and the real-Postgres suite `crates/actions/tests/coach.rs`. Out of scope: the board and pull path's `err.card` producer, the kata's own lifecycle and measurement (T5), the PM job (T3), operations (T7) and the tool platform (T13)."
      },
      "created_at": "2026-11-22T09:17:30.02765+00:00",
      "id": "01a2028b-dde6-7ed4-9e47-6e582f2341d8",
      "kind": "statement_of_architecture_work",
      "links": {},
      "meta": {},
      "phase_code": "a",
      "project_id": "01a1d039-0d33-7e7e-890e-b437b0c78db9",
      "requested_by": "01a1e607-17d9-77d3-98d3-a43a87c4633d",
      "status": "live",
      "superseded_by_id": null,
      "supersedes_id": null,
      "title": "Statement of Architecture Work \u2014 T4 Kanban coach (retrospective, U5a)",
      "updated_at": "2026-11-22T09:17:42.179607+00:00",
      "version": 2
    }
  ],
  "source": "architecture.json#/contracts"
}
```
