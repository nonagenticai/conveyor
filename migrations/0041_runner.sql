-- 0041_runner.sql — the runner substrate: one row per dispatched attempt and its artefacts
-- (ADR 0007 §Decision 3–4, issue #144 P4).
--
-- Forward only: never edited after this slice closes (CONTRACT §2).
--
-- Vocabulary, deliberately:
--   * `job` (0001_platform.sql:120) is the ledger row — the unit of work, and the only thing a
--     worker leases. `runner_run` is one ATTEMPT at that job in a Kubernetes Job, keyed
--     `(job_id, attempt)` so a superseded pod's late write can be told from a live one. The job
--     row stays the finish authority; a run row never outlives the job it belongs to.
--   * `attempt` mirrors `job.attempts` at the moment the Job was created. It is NOT an
--     independent counter: the claim statement owns that counter (crates/store/src/jobs.rs:130).
--   * `image` is the image reference as dispatched — a digest when the capability map pins one,
--     a tag otherwise. The issue's `image_digest` column is named for what it can promise.
--   * `progress` and `result` are the pod's writes through `/api/jobs/{id}/progress|result`.
--     They are jsonb because the contract (`crates/domain/src/runner.rs`) is the schema, not SQL.
--
-- No object-store credential exists in this schema: `object_key` names an object conveyor itself
-- wrote from bytes it received (no presigned URLs, ADR 0007 §Decision 4).

CREATE TABLE runner_run (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id   uuid NOT NULL DEFAULT app_current_tenant(),
    job_id      uuid NOT NULL,
    attempt     integer NOT NULL,
    image       text NOT NULL,
    namespace   text NOT NULL,
    job_name    text NOT NULL,
    -- The exact `runner/v1` instruction this attempt was dispatched with. The routes validate a
    -- pod's result against it and serve its declared inputs, so it is kept with the attempt and
    -- not re-derived from a job payload that may since have moved on.
    instruction jsonb NOT NULL,
    -- dispatched: the Kubernetes Job exists. running: the pod was observed running.
    -- reported: the pod POSTed a result; poll has not consumed it yet. succeeded/failed: poll's
    -- terminal verdict. lost: the pod died without a result and the attempt is spent.
    status      text NOT NULL DEFAULT 'dispatched'
                CHECK (status IN ('dispatched', 'running', 'reported', 'succeeded', 'failed', 'lost')),
    progress    jsonb NOT NULL DEFAULT '{}'::jsonb,
    result      jsonb,
    created_at  timestamptz NOT NULL DEFAULT now(),
    started_at  timestamptz,
    finished_at timestamptz,
    CONSTRAINT runner_run_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT runner_run_job_fk FOREIGN KEY (job_id) REFERENCES job (id) ON DELETE CASCADE,
    CONSTRAINT runner_run_attempt_key UNIQUE (job_id, attempt),
    CONSTRAINT runner_run_attempt_positive CHECK (attempt > 0),
    CONSTRAINT runner_run_image_not_blank CHECK (btrim(image) <> ''),
    CONSTRAINT runner_run_names_not_blank CHECK (btrim(namespace) <> '' AND btrim(job_name) <> ''),
    CONSTRAINT runner_run_instruction_is_object CHECK (jsonb_typeof(instruction) = 'object'),
    CONSTRAINT runner_run_progress_is_object CHECK (jsonb_typeof(progress) = 'object'),
    CONSTRAINT runner_run_result_is_object CHECK (result IS NULL OR jsonb_typeof(result) = 'object'),
    CONSTRAINT runner_run_finish_after_start CHECK (
        finished_at IS NULL OR started_at IS NULL OR finished_at >= started_at
    )
);

ALTER TABLE runner_run ENABLE ROW LEVEL SECURITY;
ALTER TABLE runner_run FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON runner_run FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- The reaper's read: live attempts by age. Partial, because a terminal row is never revisited.
CREATE INDEX runner_run_live_idx ON runner_run (created_at)
    WHERE status IN ('dispatched', 'running', 'reported');

CREATE TABLE runner_artefact (
    id           uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id    uuid NOT NULL DEFAULT app_current_tenant(),
    run_id       uuid NOT NULL,
    name         text NOT NULL,
    sha256       text NOT NULL,
    bytes        bigint NOT NULL,
    content_type text NOT NULL,
    -- The key conveyor wrote the bytes under; the bytes are in the object store, never here.
    object_key   text NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT runner_artefact_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT runner_artefact_run_fk FOREIGN KEY (run_id) REFERENCES runner_run (id) ON DELETE CASCADE,
    CONSTRAINT runner_artefact_run_name_key UNIQUE (run_id, name),
    CONSTRAINT runner_artefact_name_format CHECK (name ~ '^[a-z][a-z0-9_.-]{0,63}$'),
    CONSTRAINT runner_artefact_sha256_format CHECK (sha256 ~ '^[0-9a-f]{64}$'),
    CONSTRAINT runner_artefact_bytes_not_negative CHECK (bytes >= 0),
    CONSTRAINT runner_artefact_content_type_not_blank CHECK (btrim(content_type) <> ''),
    CONSTRAINT runner_artefact_key_not_blank CHECK (btrim(object_key) <> '')
);

ALTER TABLE runner_artefact ENABLE ROW LEVEL SECURITY;
ALTER TABLE runner_artefact FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON runner_artefact FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

CREATE INDEX runner_artefact_tenant_idx ON runner_artefact (tenant_id, run_id);

-- A runner job is claimed once to dispatch and claimed again to poll, and the second claim is a
-- CONTINUATION of the same attempt, not a retry: `runner_run.attempt` must not move or the pod's
-- idempotency key does. `continuation` is how the ledger knows not to count that claim.
--   * `defer()` sets it with the row back to `pending`;
--   * the claim increments `attempts` only when it is false, and clears it;
--   * `fail()` never sets it, so a genuine failure still spends a retry.
-- It cannot be derived from any existing column, which is why it is one.
ALTER TABLE job ADD COLUMN continuation boolean NOT NULL DEFAULT false;
