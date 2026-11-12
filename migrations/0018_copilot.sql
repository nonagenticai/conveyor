-- 0018_copilot.sql — the two S10 tables and the one index S10 is the only writer of.
--
-- Column-for-column from 02-DATA-ARCHITECTURE §2.11. Forward only: never edited after this slice closes
-- (CONTRACT §2). Declared in dependency order — `conversation` first, because `conversation_turn` names it.
--
-- The design decisions this schema carries:
--
--   * **Tool calls do not live here.** A turn's tool calls are `action_invocation` rows, referenced from
--     `conversation_turn.meta`; this pair of tables records the conversation, not a second execution log.
--   * **`seq` is the turn's identity inside a conversation.** `uq (conversation_id, seq)` is the backstop;
--     the allocator takes `SELECT … FOR UPDATE` on the conversation row, so a concurrent pair cannot both
--     read the same max and race the index (S10 §5.4).
--   * **Only committed turns are durable.** `meta.status` starts `streaming` and becomes terminal in one
--     transaction that also writes `conversation_turn.created`; a crash leaves `streaming`, and
--     `copilot.turn.reap` marks it `interrupted` — a turn never claims an uncommitted effect.
--   * **A proposal is an approval, not a table.** `uq_approval_copilot_turn` is a partial unique index over
--     an expression on S03's `approval`: one turn cannot raise two pending proposals. It belongs to S10's
--     migration because S10 is its only writer; S03 stays unaware of copilot (02-DATA-ARCHITECTURE §2.11).
--
-- Retention (data-arch §6): `conversation` 90 days, `conversation_turn` with its conversation. The rule is
-- recorded here; its producer is not part of S10's build order (the slice's only scheduled job is
-- `copilot.turn.reap`) — see 00-INDEX's S10 open items.

-- ── conversation ──────────────────────────────────────────────────────────────────────────
-- Per user; `project_id` is optional (a conversation may precede a project). `title` is display only and
-- starts empty — the UI may derive one from the first turn, but nothing in the API requires it.
CREATE TABLE conversation (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    user_id    uuid,
    project_id uuid,
    title      text NOT NULL DEFAULT '',
    status     text NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'archived')),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT conversation_user_fk FOREIGN KEY (user_id) REFERENCES app_user (id),
    CONSTRAINT conversation_project_fk FOREIGN KEY (project_id) REFERENCES project (id)
);
-- The list a user opens: their conversations, newest activity first.
CREATE INDEX conversation_user_updated_idx ON conversation (tenant_id, user_id, updated_at DESC);

-- ── conversation_turn ────────────────────────────────────────────────────────────────────
-- One row per message. `body` is the message text; `meta` carries the record of what the turn did
-- (status, model, tool_calls with invocation ids, error) — never the invocation rows themselves.
CREATE TABLE conversation_turn (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id       uuid NOT NULL DEFAULT app_current_tenant(),
    conversation_id uuid NOT NULL,
    seq             int NOT NULL CHECK (seq > 0),
    role            text NOT NULL CHECK (role IN ('user', 'assistant', 'system')),
    body            text NOT NULL,
    meta            jsonb NOT NULL DEFAULT '{}'::jsonb,
    trace_id        text,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT conversation_turn_conversation_fk
        FOREIGN KEY (conversation_id) REFERENCES conversation (id),
    CONSTRAINT conversation_turn_seq_key UNIQUE (conversation_id, seq)
);
-- `copilot.turn.reap` scans streaming turns older than 300 s, tenant-scoped.
CREATE INDEX conversation_turn_streaming_idx
    ON conversation_turn (tenant_id, created_at)
    WHERE meta->>'status' = 'streaming';

-- ── RLS: enabled AND forced on both ──────────────────────────────────────────────────────
-- FORCE is what makes the owner subject to the policy too; without it the schema owner reads every tenant's
-- rows (S03's rls_domain test asserts exactly this shape).
ALTER TABLE conversation      ENABLE ROW LEVEL SECURITY;
ALTER TABLE conversation      FORCE  ROW LEVEL SECURITY;
ALTER TABLE conversation_turn ENABLE ROW LEVEL SECURITY;
ALTER TABLE conversation_turn FORCE  ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON conversation FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
CREATE POLICY tenant_isolation ON conversation_turn FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── one pending proposal per turn ────────────────────────────────────────────────────────
-- `(tenant_id, run_id, gate)` cannot serve this: a copilot proposal has `run_id` NULL (there is no run), and
-- NULLs are distinct, so that index stops nothing. The expression is the turn the proposal came from.
CREATE UNIQUE INDEX uq_approval_copilot_turn
    ON approval (tenant_id, (meta->>'turn_id'))
    WHERE kind = 'copilot.action' AND status = 'pending';
