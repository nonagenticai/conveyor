-- 0026_s19_eval.sql — the eleven S19 eval tables, plus the `approval.eval_id` sign-off column.
--
-- Column-for-column from 02-DATA-ARCHITECTURE §2.19 (S19) and §2.20 (S20). Forward only: never
-- edited after this slice closes (CONTRACT §2).
--
-- The design decisions this schema carries:
--
--   * **The instrument is versioned.** `eval` is the logical flow; `eval_version` freezes its case
--     set, metrics, graders, prices and harness paths under one `digest`, and `eval_run` names a
--     version. A comparison across versions is refused (CONTRACT §20), because scores taken before
--     and after a grader change are not comparable (`eval-audit.md` §2).
--   * **A sign-off is bound to an artifact, not to a moment.** The two literal pauses are ordinary
--     `approval` rows at `eval_inputs` and `eval_grading`, carrying the instrument digest in
--     `meta.artifact_sha256`; editing the instrument voids them rather than leaving them quietly
--     valid. `eval_runs.start` recomputes readiness from those rows — there is deliberately no
--     `status='ready'` that could disagree with them.
--   * **`uq_appr_pending` cannot cover this.** It is `(tenant_id, run_id, gate)`, `run_id` is NULL
--     for an eval gate, and Postgres treats NULLs as distinct — so two pending sign-offs on one
--     eval would be accepted. Hence `eval_id` and its own partial unique index below. Measured on
--     a throwaway Postgres 18 on 2026-11-09: without this, two pending `eval_inputs` gates both
--     insert, and a probe that merely checks "the second failed" passes for the wrong reason
--     (`column "eval_id" does not exist`). Assert the constraint name, never the outcome alone.
--   * **The answer key is write-only.** `eval_case.expected` is read by the grader and nothing else;
--     no action returns it, and the subject-input builder has no field to carry it.
--   * **A failed attempt cannot occupy a slot.** `eval_attempt_error` is append-only and does NOT
--     share `eval_case_result`'s `(eval_run, case, rep)` uniqueness, so resume re-runs exactly what
--     produced no scorable output and never re-uses a row whose score and trace came from different
--     calls.
--   * **Nothing stores cost.** It is derived at read time from `inference_call` plus the judge's own
--     `judge_model`/`judge_usage` (SPEC D23). A maintained counter would drift from the rows it counts.
--   * **No maintained case counts.** `eval_version` carries no `case_count`/`train_count`; the split
--     lock is the fact and the counts are read-time queries.
--
-- Grants are NOT repeated here: `apps/server`'s `migrate` re-applies `scripts/grants-runtime.sql`
-- after the last migration, which is what makes a later slice's tables visible to `conveyor_runtime`
-- (0001's comment states the rule, and 0013 has no grant block either).

-- ── the sign-off discriminant (CCR 5 — APPROVED by the owner, 2026-11-09) ───────────────────
--
-- S03 owns `approval`; this is the additive, nullable column that lets an eval gate use the one
-- escalation primitive instead of inventing a second store (CONTRACT §20). The owner deferred the
-- CCR to the S19 build session and approved it there; `00-INDEX.md`'s decision register carries the
-- ruling. Additive and nullable: no existing row, query or index changes.
ALTER TABLE approval ADD COLUMN eval_id uuid;

-- The guarantee, not a check-then-insert. Partial (only pending rows), and only when `eval_id` is
-- set, so every existing and future run-scoped gate is unaffected.
CREATE UNIQUE INDEX uq_appr_eval_pending
    ON approval (tenant_id, eval_id, gate) WHERE status = 'pending' AND eval_id IS NOT NULL;

-- ── eval ─────────────────────────────────────────────────────────────────────────────────────
CREATE TABLE eval (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    name          text NOT NULL CHECK (name <> ''),
    purpose       text NOT NULL DEFAULT '',
    subject_kind  text NOT NULL
                  CHECK (subject_kind IN ('completions','copilot','procedure','external','agent_run')),
    entry_action  text NOT NULL,
    model_ref     text,
    prompt_ref    text,
    inputs_schema jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (length(inputs_schema::text) <= 8192),
    trace_source  text NOT NULL DEFAULT 'synthesized'
                  CHECK (trace_source IN ('synthesized','conversation_turn','run_step')),
    -- 'draft' | 'running' | 'climbing' | 'retired'. There is deliberately NO 'ready':
    -- readiness is computed from the two approvals at start time, so no column can
    -- disagree with the sign-off rows it summarises.
    status        text NOT NULL DEFAULT 'draft'
                  CHECK (status IN ('draft','running','climbing','retired')),
    current_version_id uuid,
    created_by    uuid,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX eval_tenant_idx ON eval (tenant_id, created_at DESC, id);

-- ── eval_version ─────────────────────────────────────────────────────────────────────────────
CREATE TABLE eval_version (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id       uuid NOT NULL DEFAULT app_current_tenant(),
    eval_id         uuid NOT NULL,
    version         int  NOT NULL CHECK (version >= 1),
    digest          text NOT NULL,
    -- NO case_count / train_count: a maintained counter drifts from the rows it counts.
    split_sha256    text,
    split_locked_at timestamptz,
    prices          jsonb NOT NULL DEFAULT '{}'::jsonb,
    harness_paths   text[] NOT NULL DEFAULT '{}',
    created_by      uuid,
    created_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT eval_version_eval_fk FOREIGN KEY (eval_id) REFERENCES eval (id),
    CONSTRAINT eval_version_unique UNIQUE (tenant_id, eval_id, version),
    -- a locked split names when it was drawn; readiness is the predicate, not this table
    CONSTRAINT eval_version_split_pair CHECK ((split_sha256 IS NULL) = (split_locked_at IS NULL))
);
CREATE INDEX eval_version_eval_idx ON eval_version (tenant_id, eval_id, version DESC);

-- ── eval_case ────────────────────────────────────────────────────────────────────────────────
CREATE TABLE eval_case (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id       uuid NOT NULL DEFAULT app_current_tenant(),
    version_id      uuid NOT NULL,
    ordinal         int  NOT NULL CHECK (ordinal >= 0),
    input           text NOT NULL CHECK (input <> ''),
    tags            text[] NOT NULL DEFAULT '{}',
    expected        jsonb,
    attachments     jsonb NOT NULL DEFAULT '[]'::jsonb,
    source          text NOT NULL DEFAULT 'handwritten'
                    CHECK (source IN ('production','ticket','handwritten','synthesized')),
    gold_provenance text NOT NULL DEFAULT 'human_written'
                    CHECK (gold_provenance IN ('human_written','human_verified','model')),
    gold_model      text,
    split           text NOT NULL DEFAULT 'none' CHECK (split IN ('train','test','none')),
    meta            jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT eval_case_version_fk FOREIGN KEY (version_id) REFERENCES eval_version (id),
    CONSTRAINT eval_case_ordinal_unique UNIQUE (tenant_id, version_id, ordinal),
    -- model-derived gold names its model; a golden set that cannot name its source cannot be trusted
    CONSTRAINT eval_case_gold_model CHECK (gold_provenance <> 'model' OR gold_model IS NOT NULL),
    CONSTRAINT eval_case_attachments_cap CHECK (length(attachments::text) <= 16384)
);
CREATE INDEX eval_case_version_idx ON eval_case (tenant_id, version_id, ordinal);
CREATE INDEX eval_case_split_idx   ON eval_case (tenant_id, version_id, split);

-- ── eval_metric ──────────────────────────────────────────────────────────────────────────────
CREATE TABLE eval_metric (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    version_id  uuid NOT NULL,
    key         text NOT NULL CHECK (key ~ '^[a-z][a-z0-9_]*$'),
    role        text NOT NULL CHECK (role IN ('quality','perf')),
    kind        text NOT NULL CHECK (kind IN ('binary','judge','float')),
    better      text NOT NULL DEFAULT 'higher' CHECK (better IN ('higher','lower')),
    -- the report legend truncates; qualifiers belong in the rubric, not the label
    label       text NOT NULL CHECK (char_length(label) <= 14),
    unit        text,
    grader_kind text NOT NULL CHECK (grader_kind IN ('programmatic','pairwise','rubric','human','none')),
    grader      jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (length(grader::text) <= 8192),
    rubric      text,
    ordinal     int  NOT NULL CHECK (ordinal >= 0),
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT eval_metric_version_fk FOREIGN KEY (version_id) REFERENCES eval_version (id),
    CONSTRAINT eval_metric_key_unique UNIQUE (tenant_id, version_id, key),
    -- a perf field is measured, never graded
    CONSTRAINT eval_metric_perf_ungraded CHECK (role = 'quality' OR grader_kind = 'none'),
    -- a rubric or pairwise grader always has its rubric text
    CONSTRAINT eval_metric_rubric_required
        CHECK (grader_kind NOT IN ('rubric','pairwise') OR (rubric IS NOT NULL AND rubric <> ''))
);
CREATE INDEX eval_metric_version_idx ON eval_metric (tenant_id, version_id, ordinal);

-- ── eval_run ─────────────────────────────────────────────────────────────────────────────────
CREATE TABLE eval_run (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    eval_id       uuid NOT NULL,
    version_id    uuid NOT NULL,
    -- the report builder silently ignores anything else; here it is impossible
    variant       text NOT NULL CHECK (variant = 'baseline' OR variant ~ '^v[0-9]+$'),
    round         int  NOT NULL DEFAULT 0 CHECK (round >= 0),
    mode          text NOT NULL DEFAULT 'full' CHECK (mode IN ('pilot','full')),
    reps          int  NOT NULL DEFAULT 1 CHECK (reps >= 1),
    case_count    int  NOT NULL DEFAULT 0 CHECK (case_count >= 0),
    status        text NOT NULL DEFAULT 'queued'
                  CHECK (status IN ('queued','running','partial','complete','void','cancelled')),
    change_md     text,
    change_patch  text,
    summary       jsonb NOT NULL DEFAULT '{}'::jsonb,
    narrative_md  text,
    environment   jsonb NOT NULL DEFAULT '{}'::jsonb,
    noise_floor   numeric,
    cost_ceiling_cents int CHECK (cost_ceiling_cents >= 0),
    last_polled_at timestamptz,
    started_at    timestamptz,
    finished_at   timestamptz,
    created_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT eval_run_eval_fk    FOREIGN KEY (eval_id)    REFERENCES eval (id),
    CONSTRAINT eval_run_version_fk FOREIGN KEY (version_id) REFERENCES eval_version (id),
    CONSTRAINT eval_run_variant_unique UNIQUE (tenant_id, eval_id, variant),
    CONSTRAINT eval_run_patch_cap CHECK (change_patch IS NULL OR length(change_patch) <= 262144)
);
CREATE INDEX eval_run_eval_idx   ON eval_run (tenant_id, eval_id, round DESC);
CREATE INDEX eval_run_status_idx ON eval_run (tenant_id, status) WHERE status IN ('queued','running','partial');

-- ── eval_case_result ─────────────────────────────────────────────────────────────────────────
CREATE TABLE eval_case_result (
    id                   uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id            uuid NOT NULL DEFAULT app_current_tenant(),
    eval_run_id          uuid NOT NULL,
    case_id              uuid NOT NULL,
    rep                  int  NOT NULL CHECK (rep >= 0),
    status               text NOT NULL CHECK (status IN ('ok','truncated','pending_label')),
    stop_reason          text,
    grade                jsonb NOT NULL,
    explanation          jsonb NOT NULL DEFAULT '{}'::jsonb,
    model                text NOT NULL,
    usage                jsonb NOT NULL DEFAULT '{}'::jsonb,
    latency_ms           int  NOT NULL DEFAULT 0 CHECK (latency_ms >= 0),
    tool_calls           int  NOT NULL DEFAULT 0 CHECK (tool_calls >= 0),
    action_invocation_id uuid,
    subject_run_id       uuid,
    judge_model          text,
    judge_usage          jsonb,
    human_score          numeric,
    created_at           timestamptz NOT NULL DEFAULT now(),
    -- ONE row per slot, and a failed attempt never writes here (see eval_attempt_error)
    CONSTRAINT eval_case_result_slot UNIQUE (tenant_id, eval_run_id, case_id, rep),
    -- with more than one quality metric the grade MUST be the dict form, or every metric column
    -- renders as a dash and is read as a zero
    CONSTRAINT eval_case_result_grade_object CHECK (jsonb_typeof(grade) = 'object'),
    CONSTRAINT eval_case_result_label CHECK (status <> 'pending_label' OR human_score IS NULL)
);
CREATE INDEX eval_case_result_run_idx  ON eval_case_result (tenant_id, eval_run_id, case_id, rep);
CREATE INDEX eval_case_result_slow_idx ON eval_case_result (tenant_id, eval_run_id, latency_ms DESC);

-- ── eval_attempt_error ───────────────────────────────────────────────────────────────────────
CREATE TABLE eval_attempt_error (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    eval_run_id   uuid NOT NULL,
    case_id       uuid NOT NULL,
    rep           int  NOT NULL CHECK (rep >= 0),
    attempt       int  NOT NULL CHECK (attempt >= 1),
    failure_class text NOT NULL
                  CHECK (failure_class IN ('refusal','harness','serving','timeout',
                                           'served_model_mismatch','grader')),
    retry_count   int  NOT NULL DEFAULT 0 CHECK (retry_count >= 0),
    model         text,
    usage         jsonb,
    message       text NOT NULL,
    deadline_at   timestamptz,
    created_at    timestamptz NOT NULL DEFAULT now(),
    -- APPEND-ONLY across resumes: one row per failed attempt. A later success writes
    -- eval_case_result and leaves these in place, superseded but never deleted.
    CONSTRAINT eval_attempt_unique UNIQUE (tenant_id, eval_run_id, case_id, rep, attempt)
);
CREATE INDEX eval_attempt_error_run_idx ON eval_attempt_error (tenant_id, eval_run_id, failure_class);

-- ── eval_trace ───────────────────────────────────────────────────────────────────────────────
CREATE TABLE eval_trace (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    eval_run_id uuid NOT NULL,
    case_id     uuid NOT NULL,
    rep         int  NOT NULL CHECK (rep >= 0),
    turns       jsonb NOT NULL,
    sha256      text NOT NULL,
    bytes       int  NOT NULL CHECK (bytes >= 0 AND bytes <= 1048576),
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT eval_trace_slot UNIQUE (tenant_id, eval_run_id, case_id, rep),
    CONSTRAINT eval_trace_turns_array CHECK (jsonb_typeof(turns) = 'array'),
    -- a role outside the method's five is a writer bug, not data
    CONSTRAINT eval_trace_roles CHECK (NOT jsonb_path_exists(turns, '$[*] ? (@.role != "system" &&
        @.role != "user" && @.role != "assistant" && @.role != "tool_call" && @.role != "tool_result")'))
);

-- ── eval_audit ───────────────────────────────────────────────────────────────────────────────
CREATE TABLE eval_audit (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    eval_id       uuid NOT NULL,
    version_id    uuid NOT NULL,
    target_run_id uuid,
    status        text NOT NULL DEFAULT 'queued'
                  CHECK (status IN ('queued','running','succeeded','failed')),
    oracle_score  numeric,
    null_score    numeric,
    noise_floor   numeric,
    headroom      numeric,
    summary       jsonb NOT NULL DEFAULT '{}'::jsonb,
    error         text,
    requested_by  uuid,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT eval_audit_eval_fk FOREIGN KEY (eval_id) REFERENCES eval (id)
);
CREATE INDEX eval_audit_eval_idx ON eval_audit (tenant_id, eval_id, created_at DESC);

-- ── eval_audit_finding ───────────────────────────────────────────────────────────────────────
CREATE TABLE eval_audit_finding (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    audit_id    uuid NOT NULL,
    section     text NOT NULL CHECK (section IN ('1','2','3','4','5','6')),
    item        text NOT NULL,
    severity    text NOT NULL CHECK (severity IN ('misleading','noise','generality')),
    evidence    text NOT NULL CHECK (evidence <> ''),
    suggestion  text NOT NULL CHECK (suggestion <> ''),
    status      text NOT NULL DEFAULT 'open'
                CHECK (status IN ('open','accepted','fixed','dismissed')),
    decided_by  uuid,
    decided_at  timestamptz,
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT eval_audit_finding_audit_fk FOREIGN KEY (audit_id) REFERENCES eval_audit (id),
    CONSTRAINT eval_audit_finding_decider CHECK (status = 'open' OR decided_by IS NOT NULL)
);
CREATE INDEX eval_audit_finding_audit_idx ON eval_audit_finding (tenant_id, audit_id, severity, id);

-- ── eval_climb (S20, §2.20) ──────────────────────────────────────────────────────────────────
--
-- The loop state the `eval-hillclimb` method keeps in `_state.json`. Rounds are `eval_run` rows
-- with `variant='v<N>'`; this table holds only the goal, the stopping condition and best-so-far.
-- There is NO `spend` column: cumulative spend is summed from the climb's result rows and the
-- billed `usage` on failed attempts, because a maintained counter drifts from what it counts.
CREATE TABLE eval_climb (
    id                 uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id          uuid NOT NULL DEFAULT app_current_tenant(),
    eval_id            uuid NOT NULL,
    version_id         uuid NOT NULL,
    goal               jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (length(goal::text) <= 8192),
    direction          text NOT NULL DEFAULT 'higher' CHECK (direction IN ('higher','lower')),
    reps               int  NOT NULL DEFAULT 1 CHECK (reps >= 1),
    stopping           text NOT NULL DEFAULT 'plateau' CHECK (stopping IN ('plateau','rounds','manual')),
    plateau_k          int  NOT NULL DEFAULT 3 CHECK (plateau_k >= 3),  -- two flat rounds is too few
    max_rounds         int  CHECK (max_rounds >= 1),
    delta              numeric NOT NULL CHECK (delta > 0),              -- set above the noise floor
    off_limits         text[] NOT NULL DEFAULT '{}',
    approve_each_round boolean NOT NULL DEFAULT true,
    cost_ceiling_cents int CHECK (cost_ceiling_cents >= 0),
    current_round      int  NOT NULL DEFAULT 0 CHECK (current_round >= 0),
    best_round         int,
    best_test_score    numeric,
    status             text NOT NULL DEFAULT 'planned'
                       CHECK (status IN ('planned','running','plateaued','stopped')),
    created_by         uuid,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT eval_climb_eval_fk    FOREIGN KEY (eval_id)    REFERENCES eval (id),
    CONSTRAINT eval_climb_version_fk FOREIGN KEY (version_id) REFERENCES eval_version (id),
    CONSTRAINT eval_climb_rounds CHECK (stopping <> 'rounds' OR max_rounds IS NOT NULL)
);
CREATE INDEX eval_climb_eval_idx ON eval_climb (tenant_id, eval_id, created_at DESC);

-- ── forced RLS on all eleven ─────────────────────────────────────────────────────────────────
--
-- ENABLE turns the policy on; FORCE makes the *owner* subject to it too — without FORCE the schema
-- owner reads every tenant's rows and an isolation test running as the owner passes while
-- production leaks (0001's comment; S03's `rls_domain` asserts the shape).
ALTER TABLE eval                ENABLE ROW LEVEL SECURITY;
ALTER TABLE eval                FORCE  ROW LEVEL SECURITY;
ALTER TABLE eval_version        ENABLE ROW LEVEL SECURITY;
ALTER TABLE eval_version        FORCE  ROW LEVEL SECURITY;
ALTER TABLE eval_case           ENABLE ROW LEVEL SECURITY;
ALTER TABLE eval_case           FORCE  ROW LEVEL SECURITY;
ALTER TABLE eval_metric         ENABLE ROW LEVEL SECURITY;
ALTER TABLE eval_metric         FORCE  ROW LEVEL SECURITY;
ALTER TABLE eval_run            ENABLE ROW LEVEL SECURITY;
ALTER TABLE eval_run            FORCE  ROW LEVEL SECURITY;
ALTER TABLE eval_case_result    ENABLE ROW LEVEL SECURITY;
ALTER TABLE eval_case_result    FORCE  ROW LEVEL SECURITY;
ALTER TABLE eval_attempt_error  ENABLE ROW LEVEL SECURITY;
ALTER TABLE eval_attempt_error  FORCE  ROW LEVEL SECURITY;
ALTER TABLE eval_trace          ENABLE ROW LEVEL SECURITY;
ALTER TABLE eval_trace          FORCE  ROW LEVEL SECURITY;
ALTER TABLE eval_audit          ENABLE ROW LEVEL SECURITY;
ALTER TABLE eval_audit          FORCE  ROW LEVEL SECURITY;
ALTER TABLE eval_audit_finding  ENABLE ROW LEVEL SECURITY;
ALTER TABLE eval_audit_finding  FORCE  ROW LEVEL SECURITY;
ALTER TABLE eval_climb          ENABLE ROW LEVEL SECURITY;
ALTER TABLE eval_climb          FORCE  ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON eval FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON eval_version FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON eval_case FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON eval_metric FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON eval_run FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON eval_case_result FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON eval_attempt_error FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON eval_trace FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON eval_audit FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON eval_audit_finding FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON eval_climb FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
