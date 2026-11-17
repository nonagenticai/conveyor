-- 0048_pm_job.sql — T3: the PM job (SPEC_V2 §6.1, §5.4, §8.4; GitHub issue #74).
--
-- Forward only: never edited after this slice closes (CONTRACT §2).
--
-- The boundary is the T3 row and issue #74's Out of scope:
--   * the plan spine (T1, 0044) already carries the WBS, the baselines and the six-kind
--     register; this migration adds ONE reference, the change request that advanced a baseline
--     (P3), because the issue requires the two records to cite each other.
--   * the charter and closure are `governed_asset` rows whose kinds 0044 already widened
--     (charter, closure); no new table and no new kind is added here.
--   * the gate's typed evidence is stored in `approval.meta.evidence`; the approval row is
--     0002's and needs no new column.
--
-- A baseline taken without a change request has `change_id IS NULL`; a re-baseline under
-- change control cites the approved register item (`project_register_item.kind = 'change'`).

ALTER TABLE project_baseline
    ADD COLUMN change_id uuid,
    ADD CONSTRAINT project_baseline_change_fk
        FOREIGN KEY (change_id) REFERENCES project_register_item (id);

CREATE INDEX project_baseline_change_idx
    ON project_baseline (tenant_id, change_id)
    WHERE change_id IS NOT NULL;
