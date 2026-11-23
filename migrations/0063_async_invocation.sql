-- 0062_async_invocation.sql — the async invocation record (#579).
--
-- A caller that cannot hold one HTTP response open — anything behind Cloudflare's ~100 s
-- first-byte cap — submits a long action and polls the invocation id. The ESTATE ALREADY DOES
-- THIS: w2v answers 202 for an image ("202, not 200: an image is minutes of GPU work"), agent
-- for eval runs, mcp for runs, coding for jobs. conveyor had no equivalent and this is it.
--
-- Why a table rather than the idempotency row: the `idempotency_key` row carries the CALLER's
-- dedupe contract (keyed by the caller's key, scoped to an action), while this carries the
-- ENGINE's job state (keyed by a server id, addressable without knowing the action). Folding
-- the second into the first would make a poll depend on the caller having supplied a key, and
-- would delete the response when the caller's row expires.
--
-- `idempotency_key` is here so an async submit replays the first invocation id instead of
-- starting a second render: the partial unique index makes that one statement.

CREATE TABLE async_invocation (
    id              uuid PRIMARY KEY,
    tenant_id       uuid NOT NULL DEFAULT app_current_tenant(),
    action          text NOT NULL,
    actor_id        uuid NOT NULL,
    input_hash      text NOT NULL,
    -- The caller's Idempotency-Key, when it sent one. NULL is not a conflict.
    idempotency_key text,
    status          text NOT NULL DEFAULT 'running'
                    CHECK (status IN ('running', 'done', 'failed')),
    -- The status and body the synchronous call would have answered. NULL while running.
    response_status int,
    response_body   jsonb,
    version         bigint NOT NULL DEFAULT 1,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    expires_at      timestamptz NOT NULL,
    CONSTRAINT async_invocation_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id)
        ON DELETE CASCADE,
    CONSTRAINT async_invocation_done_has_a_response
        CHECK (status = 'running' OR (response_status IS NOT NULL AND response_body IS NOT NULL))
);

-- One live invocation per (tenant, action, caller key). Partial: a submit without a key is
-- always a new invocation.
CREATE UNIQUE INDEX async_invocation_key_once
    ON async_invocation (tenant_id, action, idempotency_key)
    WHERE idempotency_key IS NOT NULL;

-- The sweep's only predicate.
CREATE INDEX async_invocation_expiry ON async_invocation (expires_at);

ALTER TABLE async_invocation ENABLE ROW LEVEL SECURITY;
ALTER TABLE async_invocation FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON async_invocation FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

GRANT SELECT, INSERT, UPDATE, DELETE ON async_invocation TO conveyor_runtime;
