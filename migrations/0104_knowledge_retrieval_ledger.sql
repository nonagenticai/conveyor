-- 0104_knowledge_retrieval_ledger.sql — T11 (v1-w1c-6): the pack-hit source behind
-- `knowledge.pack_hit_rate`.
--
-- Each `knowledge.query` that reaches the retriever writes exactly one row here: how many
-- documents the packs returned and whether any pack contributed. The rate the flow metric
-- reports is "retrievals with at least one hit / all retrievals" over the window — a
-- denominator the inference ledger cannot supply (it records the prompt, not the pack hit).
--
-- Forward only: never edited after this slice closes.

CREATE TABLE knowledge_retrieval (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id     uuid NOT NULL DEFAULT app_current_tenant(),
    capability_id uuid NOT NULL,
    packs         integer NOT NULL DEFAULT 0 CHECK (packs >= 0),
    hits          integer NOT NULL DEFAULT 0 CHECK (hits >= 0),
    degraded      boolean NOT NULL DEFAULT false,
    run_id        uuid,
    created_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT knowledge_retrieval_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id)
        ON DELETE CASCADE
    -- No capability FK on purpose: the row is a measurement of what the retriever returned, and
    -- it outlives a retired capability the way an audit row does.
);

CREATE INDEX knowledge_retrieval_window_idx
    ON knowledge_retrieval (tenant_id, created_at);

ALTER TABLE knowledge_retrieval ENABLE ROW LEVEL SECURITY;
ALTER TABLE knowledge_retrieval FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON knowledge_retrieval FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
