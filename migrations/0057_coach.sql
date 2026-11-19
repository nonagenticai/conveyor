-- 0057_coach.sql — the kanban coach's records (SPEC_V2 §6.2; issue #75, T4).
--
-- Two tables, one record each:
--
--   coach_interruption  one admitted interruption: which coach card carries it, which card and
--                       run it came from, the signature it repeats on, the closed-set triage and
--                       its confidence, the window it stays open for, and the escalation/kata
--                       links. One open row per `(card, signature)` — the partial unique index is
--                       the dedupe, so a repeat block cannot open a second card.
--   coach_remedy        the at-most-one remedy action per interruption, from the closed set.
--                       Append-only: a remedy is written once and is never edited or erased.
--
-- The coach CARD is an ordinary `card` row of the catalogue type `coach` (decision Q1, issue
-- #75): same board as the card it coaches, one place a person looks. No new card table.
--
-- Forward only: never edited after this slice closes (CONTRACT §2).

-- ── coach_interruption — one admitted interruption ───────────────────────────────────────
CREATE TABLE coach_interruption (
    id                  uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id           uuid NOT NULL DEFAULT app_current_tenant(),
    -- The coach card this interruption is carried by (a `card` row of type `coach`).
    coach_card_id       uuid NOT NULL,
    -- The card whose run failed.
    interrupted_card_id uuid NOT NULL,
    failed_run_id       uuid NOT NULL,
    -- The repeat key: `(card, error class, failing process/step)`, rendered once at insert
    -- (decision Q2). Compared as text so the partial index below is a plain btree.
    signature           text NOT NULL,
    -- The same tuple, unrendered, for a reader that needs the parts.
    signature_parts     jsonb NOT NULL DEFAULT '{}'::jsonb,
    -- The closed triage set (§6.2). NULL until the coach run has triaged.
    triage_class        text
                        CHECK (triage_class IN ('transient', 'conveyor_defect', 'job_defect',
                                                'project_decision', 'external')),
    confidence          numeric(4, 3) CHECK (confidence >= 0 AND confidence <= 1),
    narrative           text,
    state               text NOT NULL DEFAULT 'open'
                        CHECK (state IN ('open', 'remedied', 'escalated', 'closed')),
    -- The repeat window (decision Q3: tenant configuration, read from the decision policy).
    window_ends_at      timestamptz NOT NULL,
    escalated_at        timestamptz,
    -- The escalation approval (a repeat signature) or the `ask` gate a low confidence opens.
    approval_id         uuid,
    -- The kata run a systemic class opened (exactly one, §6.2).
    kata_candidate_id   uuid,
    closed_at           timestamptz,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT coach_interruption_tenant_fk
        FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT coach_interruption_coach_card_fk
        FOREIGN KEY (coach_card_id) REFERENCES card (id),
    CONSTRAINT coach_interruption_interrupted_card_fk
        FOREIGN KEY (interrupted_card_id) REFERENCES card (id),
    CONSTRAINT coach_interruption_run_fk
        FOREIGN KEY (failed_run_id) REFERENCES run (id),
    CONSTRAINT coach_interruption_signature_not_blank CHECK (btrim(signature) <> ''),
    CONSTRAINT coach_interruption_parts_object
        CHECK (jsonb_typeof(signature_parts) = 'object'),
    -- A triage is the class and its evidence together: a class with no confidence is a claim
    -- nobody can weigh, and a confidence with no class has nothing to weigh.
    CONSTRAINT coach_interruption_triage_together
        CHECK ((triage_class IS NULL AND confidence IS NULL)
               OR (triage_class IS NOT NULL AND confidence IS NOT NULL))
);

-- One OPEN interruption per (card, signature). A closed row (window passed, or explicitly
-- closed) no longer blocks a new one, which is what makes a post-window repeat legitimate.
CREATE UNIQUE INDEX uq_coach_interruption_open
    ON coach_interruption (tenant_id, interrupted_card_id, signature)
    WHERE closed_at IS NULL;
CREATE INDEX coach_interruption_card_idx
    ON coach_interruption (tenant_id, interrupted_card_id, created_at DESC);
CREATE INDEX coach_interruption_coach_card_idx
    ON coach_interruption (tenant_id, coach_card_id);

ALTER TABLE coach_interruption ENABLE ROW LEVEL SECURITY;
ALTER TABLE coach_interruption FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON coach_interruption FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── coach_remedy — at most one remedy per interruption ───────────────────────────────────
CREATE TABLE coach_remedy (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id       uuid NOT NULL DEFAULT app_current_tenant(),
    interruption_id uuid NOT NULL,
    -- The closed remedy set (§6.2). `retry` is the SPEC_V2 spelling of the SOP's "rerun".
    remedy_class    text NOT NULL
                    CHECK (remedy_class IN ('retry', 'move', 'repull', 'block', 'ask', 'note')),
    detail          jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT coach_remedy_tenant_fk
        FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT coach_remedy_interruption_fk
        FOREIGN KEY (interruption_id) REFERENCES coach_interruption (id),
    CONSTRAINT coach_remedy_detail_object CHECK (jsonb_typeof(detail) = 'object'),
    CONSTRAINT coach_remedy_one_per_interruption UNIQUE (tenant_id, interruption_id)
);

CREATE INDEX coach_remedy_interruption_idx ON coach_remedy (tenant_id, interruption_id);

ALTER TABLE coach_remedy ENABLE ROW LEVEL SECURITY;
ALTER TABLE coach_remedy FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON coach_remedy FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
