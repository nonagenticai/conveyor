-- 0068_development_project.sql — T6: the development project (SPEC_V2 §5.2, §5.4, §6.4,
-- §9.1, §9.3, §12 T6; issue #80).
--
-- Forward only: never edited after this slice closes (CONTRACT §2).
--
-- A development project is an ordinary `project` row with a `development` type and a stricter
-- gate set (§9.1). What this migration adds is the record that makes the self-development path
-- auditable:
--
--   * `development_project` — one repo, one lane, the implementer and the gate set the project
--     is judged by. The gate set and the measuring instrument are charter fields; the project's
--     own actions never rewrite them (§9.3).
--   * `development_step`  — one immutable evidence row per step (branch, PR, CI verdict, merge,
--     rollback plan, deploy, live check, exercised rollback). The unique key is what makes
--     "no step may be skipped" a database fact: a step is recorded once and never rewritten.
--   * `development_closure` — the closure record: it links the lane, PR, CI run, reviewer,
--     merge SHA, deployed digest and live check. Append-only (grants in
--     `scripts/grants-runtime.sql`).
--
-- P2's auto-merge extends the existing `approval_policy` surface rather than forking a second
-- one (§6.4): the row gains the approver kind, the typed conditions and the expiry. A missing
-- row still means the strict default — a human decides — and `approver_kind = 'auto'` is never
-- the default, so auto-merge is off until a tenant records it.

-- ── P2: the auto-merge policy, on the one approval-policy table ──────────────────────────
ALTER TABLE approval_policy
    ADD COLUMN approver_kind text NOT NULL DEFAULT 'human',
    ADD COLUMN conditions    jsonb NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN expires_at    timestamptz;

ALTER TABLE approval_policy
    ADD CONSTRAINT approval_policy_approver_kind_vocabulary
        CHECK (approver_kind IN ('human', 'auto')),
    ADD CONSTRAINT approval_policy_conditions_is_object
        CHECK (jsonb_typeof(conditions) = 'object'),
    -- An auto policy is earned, never a blank cheque: it carries its conditions and an expiry.
    -- The action validates the condition vocabulary; this is the backstop that a row written by
    -- any path still has terms.
    ADD CONSTRAINT approval_policy_auto_requires_terms
        CHECK (approver_kind <> 'auto'
               OR (expires_at IS NOT NULL AND conditions <> '{}'::jsonb));

-- ── development_project — the charter ───────────────────────────────────────────────────
-- One development project per project row: the project is the ordinary entity, this row is the
-- development-specific charter (the repo, the lane and the gates that judge it).
CREATE TABLE development_project (
    id                 uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id          uuid NOT NULL DEFAULT app_current_tenant(),
    project_id         uuid NOT NULL,
    -- The one repository and the one lane this project delivers through (§6.4, §9.1).
    repo               text NOT NULL,
    lane               text NOT NULL,
    -- The implementing principal. It can never decide the review, merge or deploy gate that
    -- judges its work (§5.4, §9.3) — enforced by the one decision writer.
    implementer_id     uuid NOT NULL,
    -- The gate set copied into the charter before any branch. The project's own actions cannot
    -- change it (§9.3); a separate governance project does.
    gate_set           jsonb NOT NULL DEFAULT '["review", "merge", "deploy"]'::jsonb,
    -- The eval/benchmark instrument that measures the project, when one applies. Also a charter
    -- field: a project cannot swap the instrument that judges it.
    instrument_ref     text,
    -- The project's own acceptance command, recorded so the live check names what it ran.
    acceptance_command text NOT NULL,
    status             text NOT NULL DEFAULT 'chartered'
                       CHECK (status IN ('chartered', 'in_progress', 'merged', 'deployed',
                                         'verified', 'closed', 'refused')),
    created_by         uuid,
    version            bigint NOT NULL DEFAULT 1,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT development_project_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id)
        ON DELETE CASCADE,
    CONSTRAINT development_project_project_fk FOREIGN KEY (project_id) REFERENCES project (id)
        ON DELETE CASCADE,
    CONSTRAINT development_project_implementer_fk FOREIGN KEY (implementer_id) REFERENCES app_user (id),
    CONSTRAINT development_project_one_per_project UNIQUE (tenant_id, project_id),
    CONSTRAINT development_project_repo_not_blank CHECK (btrim(repo) <> ''),
    CONSTRAINT development_project_lane_not_blank CHECK (btrim(lane) <> ''),
    CONSTRAINT development_project_acceptance_not_blank CHECK (btrim(acceptance_command) <> ''),
    CONSTRAINT development_project_gate_set_is_array
        CHECK (jsonb_typeof(gate_set) = 'array')
);
CREATE INDEX development_project_tenant_idx
    ON development_project (tenant_id, status, created_at DESC, id);

ALTER TABLE development_project ENABLE ROW LEVEL SECURITY;
ALTER TABLE development_project FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON development_project FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── development_step — one immutable evidence row per step ──────────────────────────────
-- The unique key is P1.1's "no step may be skipped" made durable: a step is written once. The
-- action layer checks that every predecessor exists in the same transaction; the index is what
-- stops a second writer racing a rewrite.
CREATE TABLE development_step (
    id                     uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id              uuid NOT NULL DEFAULT app_current_tenant(),
    development_project_id uuid NOT NULL,
    step                   text NOT NULL
                           CHECK (step IN ('branch', 'pull_request', 'ci', 'merge',
                                           'rollback_plan', 'deploy', 'live_verify',
                                           'rollback_exercise')),
    evidence               jsonb NOT NULL,
    recorded_by            uuid,
    created_at             timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT development_step_project_fk FOREIGN KEY (development_project_id)
        REFERENCES development_project (id) ON DELETE CASCADE,
    CONSTRAINT development_step_one_per_step UNIQUE (tenant_id, development_project_id, step),
    CONSTRAINT development_step_evidence_is_object CHECK (jsonb_typeof(evidence) = 'object')
);
CREATE INDEX development_step_project_idx
    ON development_step (tenant_id, development_project_id, created_at);

ALTER TABLE development_step ENABLE ROW LEVEL SECURITY;
ALTER TABLE development_step FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON development_step FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── development_closure — the closure record ────────────────────────────────────────────
-- P1.1: the closure links the lane, the PR, the CI run, the reviewer's decision, the merge SHA,
-- the deployed digest and the live check. It is written once; the `closure` document is the
-- rendered record, the typed columns are the links the read model and T7's rollback action use.
CREATE TABLE development_closure (
    id                     uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id              uuid NOT NULL DEFAULT app_current_tenant(),
    development_project_id uuid NOT NULL,
    review_approval_id     uuid NOT NULL,
    merge_approval_id      uuid NOT NULL,
    deploy_approval_id     uuid NOT NULL,
    lane                   text NOT NULL,
    pr_url                 text NOT NULL,
    ci_run_id              text NOT NULL,
    ci_verdict             text NOT NULL CHECK (ci_verdict = 'green'),
    reviewer_id            uuid NOT NULL,
    merge_sha              text NOT NULL,
    deployed_digest        text NOT NULL,
    live_check             jsonb NOT NULL,
    closure                jsonb NOT NULL,
    closed_by              uuid,
    created_at             timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT development_closure_project_fk FOREIGN KEY (development_project_id)
        REFERENCES development_project (id) ON DELETE CASCADE,
    CONSTRAINT development_closure_one_per_project UNIQUE (tenant_id, development_project_id),
    CONSTRAINT development_closure_review_fk FOREIGN KEY (review_approval_id) REFERENCES approval (id),
    CONSTRAINT development_closure_merge_fk FOREIGN KEY (merge_approval_id) REFERENCES approval (id),
    CONSTRAINT development_closure_deploy_fk FOREIGN KEY (deploy_approval_id) REFERENCES approval (id),
    CONSTRAINT development_closure_reviewer_fk FOREIGN KEY (reviewer_id) REFERENCES app_user (id),
    CONSTRAINT development_closure_live_check_is_object
        CHECK (jsonb_typeof(live_check) = 'object'),
    CONSTRAINT development_closure_document_is_object CHECK (jsonb_typeof(closure) = 'object')
);
CREATE INDEX development_closure_tenant_idx
    ON development_closure (tenant_id, created_at DESC, id);

ALTER TABLE development_closure ENABLE ROW LEVEL SECURITY;
ALTER TABLE development_closure FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON development_closure FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- One pending development gate per (project, gate). The generic `uq_appr_pending` is
-- `(tenant, run_id, gate)` and treats a NULL `run_id` as distinct, so a project-scoped gate
-- needs its own guarantee: two callers racing `merge_gate` cannot open two pending gates.
CREATE UNIQUE INDEX uq_appr_development_pending
    ON approval (tenant_id, project_id, gate)
    WHERE status = 'pending' AND kind = 'development';
