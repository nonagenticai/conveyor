-- 0088_programme_risk_register.sql — a risk belongs to a project OR a programme
-- (SPEC_V2 §8.3, §8.4; ADR 0010 docs/decisions/0010-no-deferrals.md; plan v5 §4.1).
--
-- 0044's `project_register_item.project_id` was NOT NULL, so the programme's operated risk
-- register (§8.3's "existing risk/issue registers" at programme level) had nowhere to live: a
-- programme risk is not a project risk and must not be forced into a project that does not own
-- it. This migration makes the register row's scope a typed choice — exactly one of a project
-- or a programme — and adds the two fields a risk review needs: when it is next reviewed and
-- what the response is.
--
-- Forward only: never edited after this slice closes (CONTRACT §2).
--
-- The RLS policy is unchanged: the row is still tenant-scoped, whichever parent it names.

ALTER TABLE project_register_item
    ADD COLUMN programme_id uuid,
    ADD COLUMN review_at    date,
    ADD COLUMN response     text,
    ALTER COLUMN project_id DROP NOT NULL,
    ADD CONSTRAINT project_register_item_programme_fk
        FOREIGN KEY (programme_id) REFERENCES programme (id),
    ADD CONSTRAINT project_register_item_scope_one
        CHECK (num_nonnulls(project_id, programme_id) = 1);

-- A programme register is read by (tenant, kind) and reviewed by date; the project index
-- (0044) cannot serve a programme-scoped read because its leading key is project_id.
CREATE INDEX project_register_item_review_idx
    ON project_register_item (tenant_id, kind, review_at);

-- The kind vocabulary of the register is unchanged: a programme-level row is still one of the
-- same six kinds. `project_register_item_change_impact` (0044, narrowed by 0065) also stays:
-- a change row still names its impact block and the baseline it changes.
