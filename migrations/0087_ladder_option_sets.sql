-- 0087_ladder_option_sets.sql — options are authored, versioned rows; a decision pins
-- the set it was constrained to (SPEC_V2 §7.2, §10.3; finding v1-w1c-4).
--
-- Before this migration `decision_option` was only a per-decision child: the caller posted
-- the option list with the answer, `option_set_version` was fabricated as 1, and nothing
-- compared the answer to a key. A prompt could shrink or invent the option space per call.
--
-- The model, mirroring the agent platform's `job_gate_question` /
-- `job_gate_option_refinement` (0143_job_gates.sql) as far as conveyor's tenant-scoped
-- schema allows:
--
--   * `decision_option_set` — one authored set per (tenant, kind, gate, origin), versioned,
--     draft|published. `origin = 'library'` is the platform's base set; `origin = 'tenant'`
--     is the tenant's own set and shadows the library for the same case (the platform's
--     library-first / tenant-key-shadow fold). The library is a tenant-visible row rather
--     than a separate platform catalogue because every conveyor table is tenant-scoped and
--     FORCE RLS; a cross-tenant library table would be a second isolation model.
--   * `decision_option_entry` — the set's members: key, label, criteria (the description the
--     laya wire reads), position. Replacing entries is a new version of the set.
--   * `decision_option_refinement` — a tenant's refinement of ONE library entry: label,
--     criteria, position, hidden, pinned by `base_hash` (the canonical hash of the base
--     entry the tenant authored against). A refinement whose base changed is refused, never
--     silently re-applied to a different option.
--   * `decision_option` stays the per-decision immutable SNAPSHOT of the effective options.
--     It is not extended into the authored set: the two have different lifecycles (a set is
--     edited and versioned; a decision's snapshot never changes), and the snapshot is the
--     row that proves what one decision was constrained to.
--   * `decision` pins the set (`option_set_id`, `option_set_version`, `option_set_hash`) and
--     the base hash of every refinement it applied (`option_refinement_hashes`).
--   * `decision_policy_refinement.base_hash` pins the policy content a tenant refinement was
--     authored against, so a later policy edit refuses the stale refinement instead of
--     silently applying it to different thresholds.
--
-- Forward only: never edited after this slice closes (CONTRACT §2).

-- ── the authored set ──────────────────────────────────────────────────────────────────────
CREATE TABLE decision_option_set (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    kind       text NOT NULL,
    gate       text,
    origin     text NOT NULL DEFAULT 'tenant'
               CHECK (origin IN ('library', 'tenant')),
    status     text NOT NULL DEFAULT 'draft'
               CHECK (status IN ('draft', 'published')),
    version    bigint NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT decision_option_set_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT decision_option_set_kind_not_blank CHECK (btrim(kind) <> ''),
    CONSTRAINT decision_option_set_gate_not_blank CHECK (gate IS NULL OR btrim(gate) <> '')
);

-- One set per case and origin. The tenant's row is the shadow; the library's row is the base.
CREATE UNIQUE INDEX decision_option_set_case
    ON decision_option_set (tenant_id, kind, COALESCE(gate, ''), origin);

ALTER TABLE decision_option_set ENABLE ROW LEVEL SECURITY;
ALTER TABLE decision_option_set FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON decision_option_set FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── the set's members ─────────────────────────────────────────────────────────────────────
CREATE TABLE decision_option_entry (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    set_id     uuid NOT NULL,
    key        text NOT NULL,
    label      text NOT NULL DEFAULT '',
    criteria   text NOT NULL DEFAULT '',
    position   int NOT NULL CHECK (position >= 1),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT decision_option_entry_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT decision_option_entry_set_fk FOREIGN KEY (set_id) REFERENCES decision_option_set (id) ON DELETE CASCADE,
    CONSTRAINT decision_option_entry_set_key_key UNIQUE (set_id, key),
    CONSTRAINT decision_option_entry_set_position_key UNIQUE (set_id, position),
    CONSTRAINT decision_option_entry_key_not_blank CHECK (btrim(key) <> ''),
    CONSTRAINT decision_option_entry_key_length CHECK (char_length(key) <= 128)
);
CREATE INDEX decision_option_entry_set_idx ON decision_option_entry (tenant_id, set_id, position);

ALTER TABLE decision_option_entry ENABLE ROW LEVEL SECURITY;
ALTER TABLE decision_option_entry FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON decision_option_entry FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── the tenant's refinement of one library entry ─────────────────────────────────────────
-- `base_hash` is the canonical hash of the base entry (kind, gate, key, label, criteria,
-- position). The resolver recomputes it and refuses a mismatch: a refinement is valid only
-- against the base it was authored against.
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
    CONSTRAINT decision_option_refinement_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT decision_option_refinement_entry_fk FOREIGN KEY (entry_id) REFERENCES decision_option_entry (id) ON DELETE CASCADE,
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
-- `option_set_version` already exists (0046). The three new columns name the set, the
-- effective options' canonical hash, and the base hash of every refinement applied.
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
-- The snapshot keeps the effective label and the description the wire carried, in the
-- authored order; a decision read back can rebuild exactly what laya was offered.
ALTER TABLE decision_option
    ADD COLUMN criteria text NOT NULL DEFAULT '',
    ADD COLUMN position int NOT NULL DEFAULT 1,
    ADD CONSTRAINT decision_option_position_check CHECK (position >= 1);

-- ── the policy refinement's base ─────────────────────────────────────────────────────────
-- NULL is the legacy shape (a row published before this migration). `refinements.publish`
-- always writes the canonical hash of the policy it read; `effective_policy_tx` refuses a
-- refinement whose base changed.
ALTER TABLE decision_policy_refinement
    ADD COLUMN base_hash text,
    ADD CONSTRAINT decision_policy_refinement_base_hash_shape
        CHECK (base_hash IS NULL OR base_hash ~ '^[0-9a-f]{64}$');
