-- 0103_ladder_option_set_union.sql — #526: one authored option-set store on main's 0071.
--
-- History: main's 0071 (#656, S21 step 10) created `decision_option_set` and
-- `decision_option_set_option`, and added `decision_policy.fire_when`. The lane's T2 model
-- (v1-w1c-4) independently created a richer `decision_option_set` plus `decision_option_entry`
-- and `decision_option_refinement` in a parallel `0087_ladder_option_sets.sql` that never
-- reached main. The merge would otherwise leave two membership stores for one concept, which
-- T0's gate forbids ("no concept has two stores").
--
-- Main's 0071 is already applied in production with its checksum, so it stays verbatim. This
-- migration is the delta that evolves main's tables into the lane's model, and the lane's 0087
-- is retired. Any database that applied the lane-only 0087 is rebuilt from the production dump
-- (the lane's staging rule); production never saw it.
--
-- Forward only: never edited after this slice closes.

-- ── the header gains the origin and lifecycle ─────────────────────────────────────────────
ALTER TABLE decision_option_set
    ADD COLUMN origin text NOT NULL DEFAULT 'tenant'
        CHECK (origin IN ('library', 'tenant')),
    ADD COLUMN status text NOT NULL DEFAULT 'draft'
        CHECK (status IN ('draft', 'published')),
    ADD CONSTRAINT decision_option_set_gate_not_blank
        CHECK (gate IS NULL OR btrim(gate) <> '');

-- One set per (tenant, kind, gate, origin): a `library` row is the base, a `tenant` row
-- shadows it for the same case. Main's two partial indexes cannot express the origin split.
DROP INDEX uq_decision_option_set_gated;
DROP INDEX uq_decision_option_set_ungated;
CREATE UNIQUE INDEX decision_option_set_case
    ON decision_option_set (tenant_id, kind, COALESCE(gate, ''), origin);

-- ── one membership store: adopt main's child into the lane's entry shape ──────────────────
ALTER TABLE decision_option_set_option RENAME TO decision_option_entry;

ALTER TABLE decision_option_entry
    RENAME CONSTRAINT decision_option_set_option_set_fk TO decision_option_entry_set_fk;
ALTER TABLE decision_option_entry
    RENAME CONSTRAINT decision_option_set_option_key_not_blank TO decision_option_entry_key_not_blank;
ALTER TABLE decision_option_entry
    RENAME CONSTRAINT decision_option_set_option_outcome TO decision_option_entry_outcome;

ALTER TABLE decision_option_entry ADD COLUMN id uuid;
-- The backfills below must see every tenant's rows. `decision_option_entry` is FORCE RLS
-- (0071), and the migration role is the owner, so with FORCE on the UPDATE would see only the
-- unset tenant and leave the rows NULL — the same failure 0096 hit on the production dump.
ALTER TABLE decision_option_entry NO FORCE ROW LEVEL SECURITY;
UPDATE decision_option_entry SET id = uuidv7() WHERE id IS NULL;
ALTER TABLE decision_option_entry ALTER COLUMN id SET NOT NULL;

-- The lane's positions are 1-based and unique per set; main's default was 0. Renumber in
-- authored order before the constraint goes on.
UPDATE decision_option_entry e
SET position = ordered.rn
FROM (
    SELECT set_id, key,
           row_number() OVER (PARTITION BY set_id ORDER BY position, key) AS rn
    FROM decision_option_entry
) AS ordered
WHERE e.set_id = ordered.set_id AND e.key = ordered.key;
ALTER TABLE decision_option_entry FORCE ROW LEVEL SECURITY;

ALTER TABLE decision_option_entry
    ADD COLUMN criteria text NOT NULL DEFAULT '',
    ADD COLUMN updated_at timestamptz NOT NULL DEFAULT now();

ALTER TABLE decision_option_entry DROP CONSTRAINT decision_option_set_option_pkey;
ALTER TABLE decision_option_entry ADD PRIMARY KEY (id);
-- Both insert paths (S21's author_option_set_tx and the lane's insert_option_entry_tx) rely on
-- the id being generated: the lane's 0087 declared `id uuid PRIMARY KEY DEFAULT uuidv7()`, and
-- main's child had no id at all. Without the default every authoring insert violates NOT NULL.
ALTER TABLE decision_option_entry ALTER COLUMN id SET DEFAULT uuidv7();
ALTER TABLE decision_option_entry
    ADD CONSTRAINT decision_option_entry_tenant_fk
        FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    ADD CONSTRAINT decision_option_entry_set_key_key UNIQUE (set_id, key),
    ADD CONSTRAINT decision_option_entry_set_position_key UNIQUE (set_id, position),
    ADD CONSTRAINT decision_option_entry_key_length CHECK (char_length(key) <= 128),
    ADD CONSTRAINT decision_option_entry_position_check CHECK (position >= 1);

CREATE INDEX decision_option_entry_set_idx
    ON decision_option_entry (tenant_id, set_id, position);

-- ── the tenant's refinement of one library entry ─────────────────────────────────────────
CREATE TABLE decision_option_refinement (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    entry_id   uuid NOT NULL,
    label      text,
    criteria   text,
    position   int CHECK (position IS NULL OR position >= 1),
    hidden     boolean NOT NULL DEFAULT false,
    base_hash  text NOT NULL,
    version    bigint NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT decision_option_refinement_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id)
        ON DELETE CASCADE,
    CONSTRAINT decision_option_refinement_entry_fk FOREIGN KEY (entry_id) REFERENCES decision_option_entry (id)
        ON DELETE CASCADE,
    CONSTRAINT decision_option_refinement_one_per_entry UNIQUE (tenant_id, entry_id),
    CONSTRAINT decision_option_refinement_base_hash_shape CHECK (base_hash ~ '^[0-9a-f]{64}$')
);
CREATE INDEX decision_option_refinement_entry_idx
    ON decision_option_refinement (tenant_id, entry_id);

ALTER TABLE decision_option_refinement ENABLE ROW LEVEL SECURITY;
ALTER TABLE decision_option_refinement FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON decision_option_refinement FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── the decision's pins ──────────────────────────────────────────────────────────────────
-- `option_set_version` already exists (0046). These three columns name the set, the effective
-- options' canonical hash, and the base hash of every refinement applied.
ALTER TABLE decision
    ADD COLUMN option_set_id uuid,
    ADD COLUMN option_set_hash text,
    ADD COLUMN option_refinement_hashes text[] NOT NULL DEFAULT '{}';

ALTER TABLE decision
    ADD CONSTRAINT decision_option_set_fk
        FOREIGN KEY (option_set_id) REFERENCES decision_option_set (id),
    ADD CONSTRAINT decision_option_set_hash_shape
        CHECK (option_set_hash IS NULL OR option_set_hash ~ '^[0-9a-f]{64}$'),
    ADD CONSTRAINT decision_option_refinement_hashes_shape
        CHECK (array_position(option_refinement_hashes, NULL) IS NULL),
    -- A named set always pins its version and its effective hash. Rows written before this
    -- migration carry the fabricated `option_set_version = 1` with no set id; they stay
    -- readable, and no new row can repeat that shape.
    ADD CONSTRAINT decision_option_set_pin_complete
        CHECK (option_set_id IS NULL
               OR (option_set_version IS NOT NULL AND option_set_hash IS NOT NULL));

CREATE INDEX decision_option_set_decision_idx
    ON decision (option_set_id)
    WHERE option_set_id IS NOT NULL;

-- ── the per-decision snapshot gains the wire's own fields ────────────────────────────────
ALTER TABLE decision_option
    ADD COLUMN criteria text NOT NULL DEFAULT '',
    ADD COLUMN position int NOT NULL DEFAULT 1,
    ADD CONSTRAINT decision_option_position_check CHECK (position >= 1);

-- ── the policy refinement's base ─────────────────────────────────────────────────────────
ALTER TABLE decision_policy_refinement
    ADD COLUMN base_hash text,
    ADD CONSTRAINT decision_policy_refinement_base_hash_shape
        CHECK (base_hash IS NULL OR base_hash ~ '^[0-9a-f]{64}$');
