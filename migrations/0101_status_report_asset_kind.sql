-- 0101_status_report_asset_kind.sql — the PM's status report is a governed asset
-- (SPEC_V2 §6.1; conformance finding v1-p61-2).
--
-- §6.1 lists the status report among the PM's outputs, but nothing could produce one: no
-- action wrote it and the governed-asset vocabulary had no kind for it, so the report lived
-- outside the governed record. This adds the kind the `pm.status` action writes; the report
-- stays a `draft` asset, citable and gated like every other governed artefact.
--
-- This restates 0089's constraint with one kind appended — 0089 is the last migration to
-- rewrite this CHECK, so its merged vocabulary (0065's `minutes`, 0069's `lesson_source`,
-- 0070's `tool_draft`, 0044's plan kinds and the seven strategy/plan artefacts) is carried
-- forward unchanged. Forward only: never edited after this slice closes (CONTRACT §2).

ALTER TABLE governed_asset DROP CONSTRAINT governed_asset_kind_check;
ALTER TABLE governed_asset ADD CONSTRAINT governed_asset_kind_check CHECK (
    kind IN ('requirement', 'principle', 'adr', 'charter', 'closure', 'postmortem',
             'sop_source', 'mandate', 'programme_brief', 'blueprint', 'business_case',
             'refined_vision', 'benefits_strategy', 'benefits_realization_plan',
             'projects_dossier', 'minutes', 'lesson_source', 'tool_draft',
             'information_strategy', 'risk_strategy', 'issue_strategy',
             'monitoring_strategy', 'communications_strategy', 'quality_strategy',
             'resource_strategy', 'status_report')
);
