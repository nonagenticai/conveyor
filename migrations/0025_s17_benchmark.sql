-- 0025_s17_benchmark.sql — the four S17 benchmark tables.
--
-- Column-for-column from 02-DATA-ARCHITECTURE §2.18 (SPEC D24, CONTRACT §19). Forward only:
-- never edited after this slice closes (CONTRACT §2).
--
-- The design decisions this schema carries:
--
--   * **A suite is immutable identity.** `seed_sha256` + `image_lock_sha256` + `swebench_version`
--     + `parser_version` are what a number was measured against. Re-seeding the same slug with
--     different hashes is a 409, never an update (CONTRACT §19).
--   * **Answers are grader-only.** `gold_patch`, `test_patch`, `fail_to_pass`, `pass_to_pass` and
--     `eval_script` live on `benchmark_instance`; no action returns them and nothing that runs
--     model code can read them.
--   * **Runs and cases carry no `version`.** They are written only by the engine and the run
--     driver, never by an action handler — the S16 `prompt_audit` exception, stated here.
--   * **`resolved_rate` is derived**, never a column: `n_resolved / n_graded`.
--   * **`conveyor` is refused until S13/S15 land.** The columns exist as forward references
--     (no FK — CONTRACT §2's tenant rule); the CHECK forbids a `conveyor` run with a `suite`
--     subject, so an unmeasured arm cannot be recorded.
--
-- Retention (data-arch §6): instances are seeded and never pruned; runs and cases are unbounded
-- evidence; patch text is a bounded row (256 KiB) and logs are S09 documents.

-- ── benchmark_suite ────────────────────────────────────────────────────────────────────────
CREATE TABLE benchmark_suite (
    id                uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id         uuid NOT NULL DEFAULT app_current_tenant(),
    slug              text NOT NULL,
    dataset           text NOT NULL,              -- e.g. SWE-bench/SWE-bench_Verified
    dataset_revision  text NOT NULL,              -- the HF revision the seed was cut from
    seed_sha256       text NOT NULL,              -- hash of seed/benchmark/<slug>.json
    image_lock_sha256 text NOT NULL,              -- hash of data/images-<slug>.lock.json
    swebench_version  text NOT NULL,              -- the semantics the grader reproduces
    parser_version    text NOT NULL,              -- the parser set's own version
    baseline_run_id   uuid,                       -- set by benchmark_runs.promote; no FK (see below)
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT benchmark_suite_slug UNIQUE (tenant_id, slug)
);

-- ── benchmark_instance ─────────────────────────────────────────────────────────────────────
-- One seeded task. Answers live here and are read only by the grader.
CREATE TABLE benchmark_instance (
    id                uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id         uuid NOT NULL DEFAULT app_current_tenant(),
    suite_id          uuid NOT NULL REFERENCES benchmark_suite (id),
    instance_id       text NOT NULL,              -- e.g. django__django-16493
    repo              text NOT NULL,
    base_commit       text NOT NULL,
    image             text NOT NULL,              -- registry path @ sha256 digest
    problem_statement text NOT NULL,
    oracle_files      text[] NOT NULL DEFAULT '{}',  -- the paths the gold patch touches
    gold_patch        text NOT NULL,
    test_patch        text NOT NULL,
    fail_to_pass      text[] NOT NULL,
    pass_to_pass      text[] NOT NULL,
    eval_script       text NOT NULL,              -- generated once by the Rust make_test_spec port
    parser            text NOT NULL,              -- the repo's parser key
    created_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT benchmark_instance_key UNIQUE (suite_id, instance_id)
);
CREATE INDEX benchmark_instance_suite_idx ON benchmark_instance (tenant_id, suite_id);

-- ── benchmark_run ──────────────────────────────────────────────────────────────────────────
-- One measured run: arm × subject. A fresh uuid per start; starting the same suite twice is two
-- runs. `control_ok` is NULL for non-control arms and false for a control that missed.
CREATE TABLE benchmark_run (
    id                  uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id           uuid NOT NULL DEFAULT app_current_tenant(),
    suite_id            uuid NOT NULL REFERENCES benchmark_suite (id),
    arm                 text NOT NULL
                        CHECK (arm IN ('gold','empty','baseline','conveyor')),
    subject_kind        text NOT NULL DEFAULT 'suite'
                        CHECK (subject_kind IN ('suite','capability_agent','procedure')),
    capability_agent_id uuid,                    -- forward reference, no FK until S15 lands
    procedure_id        uuid,                    -- forward reference, no FK until S11's table is the target
    label               text NOT NULL,           -- the predictions label, e.g. conveyor-oracle-files+failing-test
    model               text,
    status              text NOT NULL DEFAULT 'queued'
                        CHECK (status IN ('queued','running','succeeded','failed','cancelled')),
    n                   int NOT NULL DEFAULT 0 CHECK (n >= 0),
    n_graded            int NOT NULL DEFAULT 0 CHECK (n_graded >= 0),
    n_resolved          int NOT NULL DEFAULT 0 CHECK (n_resolved >= 0),
    n_no_attempt        int NOT NULL DEFAULT 0 CHECK (n_no_attempt >= 0),
    counts              jsonb NOT NULL DEFAULT '{}',   -- the full status histogram, read whole
    control_ok          boolean,                 -- NULL for non-control arms
    cancel_requested_at timestamptz,
    last_polled_at      timestamptz,             -- the driver's heartbeat
    started_at          timestamptz,
    finished_at         timestamptz,
    error               text,
    environment         jsonb NOT NULL DEFAULT '{}',
    options             jsonb NOT NULL DEFAULT '{}',
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT benchmark_run_subject CHECK (
        (subject_kind = 'suite' AND capability_agent_id IS NULL AND procedure_id IS NULL)
        OR (subject_kind = 'capability_agent' AND capability_agent_id IS NOT NULL)
        OR (subject_kind = 'procedure' AND procedure_id IS NOT NULL)),
    CONSTRAINT benchmark_run_arm_subject CHECK (arm <> 'conveyor' OR subject_kind <> 'suite')
);
CREATE INDEX benchmark_run_tenant_created_idx ON benchmark_run (tenant_id, created_at DESC, id);
CREATE INDEX benchmark_run_suite_idx          ON benchmark_run (tenant_id, suite_id, created_at DESC);

-- ── benchmark_case ─────────────────────────────────────────────────────────────────────────
-- One instance in one run. `produce_status = no_attempt` is withheld from the graded
-- denominator; `verdict` is the grader's, never the producer's.
CREATE TABLE benchmark_case (
    id                uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id         uuid NOT NULL DEFAULT app_current_tenant(),
    benchmark_run_id  uuid NOT NULL REFERENCES benchmark_run (id),
    instance_id       text NOT NULL,
    produce_status    text NOT NULL DEFAULT 'pending'
                      CHECK (produce_status IN ('pending','ok','fail','no_attempt')),
    attempts          int NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    patch             text,
    patch_sha256      text,
    verdict           text NOT NULL DEFAULT 'pending'
                      CHECK (verdict IN ('pending','resolved','unresolved','apply_failed','eval_timeout','infra_error')),
    seconds           numeric CHECK (seconds IS NULL OR seconds >= 0),
    sandbox_ref       text,                       -- the k8s Job name
    log_document_id   uuid,                       -- S09 document; no FK (see below)
    detail            jsonb NOT NULL DEFAULT '{}',
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT benchmark_case_key UNIQUE (benchmark_run_id, instance_id),
    CONSTRAINT benchmark_case_patch_cap CHECK (patch IS NULL OR octet_length(patch) <= 262144)
);
CREATE INDEX benchmark_case_run_idx     ON benchmark_case (tenant_id, benchmark_run_id, verdict, instance_id);

-- ── RLS: enabled AND forced on all four ────────────────────────────────────────────────────
-- FORCE is what makes the owner subject to the policy too (0018's shape).
ALTER TABLE benchmark_suite    ENABLE ROW LEVEL SECURITY;
ALTER TABLE benchmark_suite    FORCE  ROW LEVEL SECURITY;
ALTER TABLE benchmark_instance ENABLE ROW LEVEL SECURITY;
ALTER TABLE benchmark_instance FORCE  ROW LEVEL SECURITY;
ALTER TABLE benchmark_run      ENABLE ROW LEVEL SECURITY;
ALTER TABLE benchmark_run      FORCE  ROW LEVEL SECURITY;
ALTER TABLE benchmark_case     ENABLE ROW LEVEL SECURITY;
ALTER TABLE benchmark_case     FORCE  ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON benchmark_suite FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON benchmark_instance FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON benchmark_run FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON benchmark_case FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
