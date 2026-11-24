-- 0069_lesson_lifecycle.sql — T8: organisational memory (SPEC_V2 §7.6, §10.3, §10.4, §12 T8,
-- D36; issue #79).
--
-- T10's 0056 created `lesson` for the retro's proposals: programme-scoped, an evidence JSON
-- array and an `independent_count`. T8 owns the lifecycle that turns a proposal into doctrine,
-- so this migration extends that one row rather than creating a second lesson table:
--
--   * a T8 proposal need not belong to a programme (a failed run or an incident is evidence in
--     its own right), so `programme_id` becomes nullable;
--   * a proposal is identified by the hash of its statement, so an identical re-proposal can
--     find the rejected row it must not silently replace;
--   * the proposer and the reviewer are recorded separately — D36's person's review is not the
--     proposer's own act;
--   * promotion names the `governed_asset` source draft it produced (the issue's open question
--     1 default: `governed_asset`, whose draft/review/approved lifecycle already exists);
--   * **exactly one independent evidence ref is refused by the database itself**: a single bad
--     run is not a lesson, and no action bug can store one.
--
-- Forward only: never edited after this slice closes (CONTRACT §2).

ALTER TABLE lesson ALTER COLUMN programme_id DROP NOT NULL;

ALTER TABLE lesson
    ADD COLUMN fingerprint text,
    ADD COLUMN proposer_id uuid,
    ADD COLUMN proposer_kind text,
    ADD COLUMN reviewed_by uuid,
    ADD COLUMN reviewed_at timestamptz,
    ADD COLUMN review_note text,
    ADD COLUMN promoted_asset_id uuid,
    ADD COLUMN promoted_at timestamptz;

-- A 0056 retro row counted its evidence array's length, so an existing one-ref row would fail
-- the new CHECK. It stays as provenance with zero independent refs (never reviewable) rather
-- than being deleted; no promotion path existed before this migration.
UPDATE lesson SET independent_count = 0 WHERE independent_count = 1;

ALTER TABLE lesson
    ADD CONSTRAINT lesson_proposer_fk FOREIGN KEY (proposer_id) REFERENCES app_user (id),
    ADD CONSTRAINT lesson_reviewer_fk FOREIGN KEY (reviewed_by) REFERENCES app_user (id),
    ADD CONSTRAINT lesson_promoted_asset_fk FOREIGN KEY (promoted_asset_id)
        REFERENCES governed_asset (id),
    ADD CONSTRAINT lesson_fingerprint_shape
        CHECK (fingerprint IS NULL OR fingerprint ~ '^[0-9a-f]{64}$'),
    ADD CONSTRAINT lesson_source_vocabulary
        CHECK (source IN ('retro', 'propose', 'execution')),
    -- P1.3: a lesson with ONE independent reference is not a lesson. Zero is allowed (a retro or
    -- an execution record that has not yet gathered its second reference); two or more is a
    -- proposal that may be reviewed.
    ADD CONSTRAINT lesson_independent_count_not_one CHECK (independent_count <> 1),
    -- P2.1: promotion always names the source draft it created, and only a lesson with two
    -- independent references can be promoted.
    ADD CONSTRAINT lesson_promoted_is_sourced
        CHECK (state <> 'promoted'
               OR (promoted_asset_id IS NOT NULL
                   AND promoted_at IS NOT NULL
                   AND independent_count >= 2)),
    -- A review decision names the person who made it; a rejection is durable evidence, not an
    -- anonymous state flip.
    ADD CONSTRAINT lesson_review_names_reviewer
        CHECK (state NOT IN ('reviewed', 'rejected')
               OR (reviewed_by IS NOT NULL AND reviewed_at IS NOT NULL))
;

-- The existing CHECK allowed 0,1,2,…; the replacement above is narrower, so remove the old one.
ALTER TABLE lesson DROP CONSTRAINT IF EXISTS lesson_independent_count_check;

-- One proposal per statement hash per tenant. Partial: a retro/execution row carries no
-- fingerprint until a person proposes it.
CREATE UNIQUE INDEX uq_lesson_fingerprint
    ON lesson (tenant_id, fingerprint)
    WHERE fingerprint IS NOT NULL;

CREATE INDEX lesson_fingerprint_idx ON lesson (tenant_id, fingerprint);
CREATE INDEX lesson_state_idx ON lesson (tenant_id, state, updated_at DESC, id);

-- ── governed_asset — the promoted lesson is a knowledge-source draft ─────────────────────
-- §10.3's governed_asset already owns the draft → in_review → approved → superseded
-- lifecycle; D36's promotion stops at `draft`, so the source is a kind, not a new table.
ALTER TABLE governed_asset DROP CONSTRAINT governed_asset_kind_check;
ALTER TABLE governed_asset ADD CONSTRAINT governed_asset_kind_check CHECK (
    kind IN ('requirement', 'principle', 'adr', 'charter', 'closure', 'postmortem',
             'sop_source', 'mandate', 'programme_brief', 'blueprint', 'business_case',
             'refined_vision', 'benefits_strategy', 'benefits_realization_plan',
             'projects_dossier', 'minutes', 'lesson_source')
);
