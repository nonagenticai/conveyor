-- S16 — the inference ledger and the prompt audit (SPEC D23, CONTRACT §18, data-arch §2.17).
--
-- The ledger is written by the engine inside `record_bookkeeping`'s transaction; no action may
-- record a call that was never made. `prompt_audit` holds one row per audit (manual or event-
-- created), `prompt_audit_finding` its findings.
--
-- All three are ENABLE + FORCE RLS with one `tenant_isolation` policy each, exactly as §2.11's
-- pair: FORCE is what makes the schema owner subject to the policy too.

CREATE TABLE inference_call (
    id                          uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id                   uuid NOT NULL DEFAULT app_current_tenant(),
    action_invocation_id        uuid,
    trace_id                    text,
    correlation_id              text,
    caller_kind                 text NOT NULL
                                CHECK (caller_kind IN ('copilot','procedure_step','workflow_step','agent_run','other')),
    run_id                      uuid,
    run_step_id                 uuid,
    conversation_turn_id        uuid,
    prompt_ref                  text,
    prompt_version              text,
    prompt_sha256               text,
    model                       text NOT NULL,
    served_model                text,
    prompt_tokens               int  NOT NULL DEFAULT 0 CHECK (prompt_tokens >= 0),
    completion_tokens           int  NOT NULL DEFAULT 0 CHECK (completion_tokens >= 0),
    total_tokens                int  NOT NULL DEFAULT 0 CHECK (total_tokens >= 0),
    cache_creation_input_tokens int  NOT NULL DEFAULT 0 CHECK (cache_creation_input_tokens >= 0),
    cache_read_input_tokens     int  NOT NULL DEFAULT 0 CHECK (cache_read_input_tokens >= 0),
    cache_status                text NOT NULL DEFAULT 'unknown'
                                CHECK (cache_status IN ('hit','write','miss','unknown')),
    latency_ms                  int  NOT NULL DEFAULT 0 CHECK (latency_ms >= 0),
    outcome                     text NOT NULL DEFAULT 'ok'
                                CHECK (outcome IN ('ok','refused','error','timeout')),
    created_at                  timestamptz NOT NULL DEFAULT now(),
    -- "whose call was this" must always be answerable (CONTRACT §18)
    CONSTRAINT inference_call_join_key CHECK (
        caller_kind = 'other'
        OR run_id IS NOT NULL OR run_step_id IS NOT NULL OR conversation_turn_id IS NOT NULL),
    -- the cache definition lives in the database too, so a second writer cannot invent a fifth meaning.
    CONSTRAINT inference_call_cache_status CHECK (
        cache_status = 'unknown'
        OR (cache_status = 'hit'   AND cache_read_input_tokens > 0)
        OR (cache_status = 'write' AND cache_read_input_tokens = 0 AND cache_creation_input_tokens > 0)
        OR (cache_status = 'miss'  AND cache_read_input_tokens = 0 AND cache_creation_input_tokens = 0))
);
CREATE INDEX inference_call_tenant_created_idx ON inference_call (tenant_id, created_at DESC, id);
CREATE INDEX inference_call_cache_idx          ON inference_call (tenant_id, cache_status, created_at DESC);
CREATE INDEX inference_call_run_idx            ON inference_call (tenant_id, run_id) WHERE run_id IS NOT NULL;
CREATE INDEX inference_call_prompt_idx         ON inference_call (tenant_id, prompt_sha256)
    WHERE prompt_sha256 IS NOT NULL;
-- The after-N-executions trigger's count: index-only, and it excludes the audit's own call so an
-- audit cannot re-trigger the cadence it is servicing.
CREATE INDEX inference_call_audit_count_idx    ON inference_call (tenant_id, created_at)
    WHERE prompt_ref IS DISTINCT FROM 'prompt_audit@1';

CREATE TABLE prompt_audit (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    scope         jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (length(scope::text) <= 8192),
    target_model  text NOT NULL,
    prompt_ref    text NOT NULL,                  -- 'prompt_audit@1'
    status        text NOT NULL DEFAULT 'queued'
                  CHECK (status IN ('queued','running','succeeded','failed')),
    finding_count int  NOT NULL DEFAULT 0 CHECK (finding_count >= 0),
    error         text,
    requested_by  uuid,                           -- NULL = the cadence trigger created it
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX prompt_audit_tenant_created_idx ON prompt_audit (tenant_id, created_at DESC, id);
-- The event trigger's race guard: at most ONE live event-created audit per tenant. Two captured
-- calls crossing the threshold together conflict here and only one inserts; a manual audit
-- (`requested_by` set) is not restricted.
CREATE UNIQUE INDEX prompt_audit_auto_live_idx ON prompt_audit (tenant_id)
    WHERE status IN ('queued','running') AND requested_by IS NULL;

CREATE TABLE prompt_audit_finding (
    id           uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id    uuid NOT NULL DEFAULT app_current_tenant(),
    audit_id     uuid NOT NULL,
    location     text NOT NULL,                   -- 'procedure:<id>' | 'procedure_step:<id>' | 'action:<name>' | 'copilot:SYSTEM_PROMPT'
    evidence     text NOT NULL,
    pattern      text NOT NULL CHECK (pattern IN ('1a','1b','1c','1d','1e','1f','2','3','4')),
    why_obsolete text NOT NULL,
    confidence   text NOT NULL CHECK (confidence IN ('high','medium','low')),
    action       text NOT NULL CHECK (action IN ('remove','rewrite','move','replace_with_api_feature','add','flag')),
    replacement  text,
    status       text NOT NULL DEFAULT 'open' CHECK (status IN ('open','accepted','rejected','applied')),
    decided_by   uuid,
    decided_at   timestamptz,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT prompt_audit_finding_audit_fk
        FOREIGN KEY (audit_id) REFERENCES prompt_audit (id),
    -- the skill's Step 6: 'rewrite' and 'add' always carry their replacement text
    CONSTRAINT prompt_audit_finding_replacement
        CHECK (action NOT IN ('rewrite','add') OR (replacement IS NOT NULL AND replacement <> '')),
    -- a decided finding names its decider (CONTRACT §18: every transition is a person's)
    CONSTRAINT prompt_audit_finding_decider
        CHECK (status = 'open' OR decided_by IS NOT NULL)
);
CREATE INDEX prompt_audit_finding_audit_idx ON prompt_audit_finding (tenant_id, audit_id, confidence, id);

-- ── RLS: enabled AND forced on all three ────────────────────────────────────────────────
ALTER TABLE inference_call       ENABLE ROW LEVEL SECURITY;
ALTER TABLE inference_call       FORCE  ROW LEVEL SECURITY;
ALTER TABLE prompt_audit         ENABLE ROW LEVEL SECURITY;
ALTER TABLE prompt_audit         FORCE  ROW LEVEL SECURITY;
ALTER TABLE prompt_audit_finding ENABLE ROW LEVEL SECURITY;
ALTER TABLE prompt_audit_finding FORCE  ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON inference_call FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON prompt_audit FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON prompt_audit_finding FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
