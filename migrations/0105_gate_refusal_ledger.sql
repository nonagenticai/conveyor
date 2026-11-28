-- 0105_gate_refusal_ledger.sql — T11 (v1-w4b-5): every refusal a gate made, so
-- `metrics.programme`'s "a gate that never refused" flag reads the refusals and not only
-- the rejected decisions.
--
-- Why a ledger: a refusal refuses the call, so the handler's transaction rolls back and any
-- row it wrote inside would vanish. The engine's failure bookkeeping runs in its own short
-- transaction after the rollback (`record_failure`) and already persists the invocation and
-- the audit event; this table is the same durable record, keyed by the gate the input named.
--
-- Forward only: never edited after this slice closes.

CREATE TABLE gate_refusal (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    gate       text NOT NULL,
    action     text NOT NULL,
    reason     text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT gate_refusal_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id)
        ON DELETE CASCADE,
    CONSTRAINT gate_refusal_gate_not_blank CHECK (btrim(gate) <> ''),
    CONSTRAINT gate_refusal_action_not_blank CHECK (btrim(action) <> '')
);

CREATE INDEX gate_refusal_window_idx
    ON gate_refusal (tenant_id, gate, created_at);

ALTER TABLE gate_refusal ENABLE ROW LEVEL SECURITY;
ALTER TABLE gate_refusal FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON gate_refusal FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
